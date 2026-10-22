package com.nexora.nxl;

import android.app.Activity;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Build;
import android.os.Bundle;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/**
 * The NEXORA mobile starter.
 *
 * <p>What is NXL and what is Android, precisely:
 *
 * <ul>
 *   <li><b>NXL</b> produces the content of each screen. Each tab holds a
 *       {@code .nxl} program in {@code assets/}; the text is passed to the NXL
 *       interpreter over JNI and what it prints is drawn.</li>
 *   <li><b>Android</b> owns the activity, the views, the scrolling, the taps,
 *       and the choice of which program a tab runs.</li>
 * </ul>
 *
 * <p>That split is the honest one. Drawing is the platform's job. The content,
 * the loops that build it and the branching that chooses it are the language's.
 *
 * <p>Everything configurable — app name, colours, tab labels, screen order,
 * initial destination, welcome copy — is generated from {@code nxl.toml}
 * {@code [mobile]} into resources and {@code assets/mobile.nxl} by
 * {@code nexora build}. This file is the part a developer edits only when
 * changing how the starter draws, not what it says.
 */
public final class MainActivity extends Activity {

    /** Written by `nexora` from nxl.toml [mobile]; prepended to each screen. */
    private static final String PRELUDE_ASSET = "mobile.nxl";

    private String[] screenLabels;
    private String[] screenAssets;
    private String prelude = "";

    private int background;
    private int surface;
    private int border;
    private int textPrimary;
    private int textSecondary;
    private int accent;
    private int secondary;

    private LinearLayout content;
    private LinearLayout tabBar;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        background = color(R.color.nxl_background);
        surface = color(R.color.nxl_surface);
        border = color(R.color.nxl_border);
        textPrimary = color(R.color.nxl_text_primary);
        textSecondary = color(R.color.nxl_text_secondary);
        accent = color(R.color.nxl_primary);
        secondary = color(R.color.nxl_secondary);

        screenLabels = getResources().getStringArray(R.array.nxl_screen_labels);
        screenAssets = getResources().getStringArray(R.array.nxl_screen_assets);
        String loaded = readAsset(PRELUDE_ASSET);
        if (loaded != null) {
            prelude = loaded;
        }

        setContentView(buildLayout());

        int initial = getResources().getInteger(R.integer.nxl_initial_screen);
        if (initial < 0 || initial >= screenLabels.length) {
            initial = 0;
        }
        showScreen(initial);
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    private View buildLayout() {
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(background);

        root.addView(buildHeader());

        ScrollView scroller = new ScrollView(this);
        scroller.setFillViewport(true);
        content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        int pad = dp(20);
        content.setPadding(pad, dp(8), pad, dp(24));
        scroller.addView(content, new ScrollView.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        root.addView(scroller, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

        root.addView(buildTabBar());
        return root;
    }

    private View buildHeader() {
        LinearLayout header = new LinearLayout(this);
        header.setOrientation(LinearLayout.HORIZONTAL);
        header.setGravity(Gravity.CENTER_VERTICAL);
        header.setBackgroundColor(surface);
        header.setPadding(dp(20), dp(18), dp(20), dp(18));

        header.addView(buildMark(), new LinearLayout.LayoutParams(dp(34), dp(34)));

        LinearLayout titles = new LinearLayout(this);
        titles.setOrientation(LinearLayout.VERTICAL);
        titles.setPadding(dp(12), 0, 0, 0);

        TextView name = new TextView(this);
        name.setText(getString(R.string.nxl_app_name));
        name.setTextSize(16);
        name.setTypeface(Typeface.DEFAULT_BOLD);
        name.setLetterSpacing(0.08f);
        name.setTextColor(textPrimary);
        titles.addView(name);

        TextView engine = new TextView(this);
        engine.setText(NxlRuntime.isAvailable()
                ? "NXL interpreter in-process"
                : "NXL library not loaded");
        engine.setTextSize(11);
        engine.setLetterSpacing(0.04f);
        engine.setTextColor(NxlRuntime.isAvailable() ? textSecondary : 0xFFFF6B6B);
        titles.addView(engine);

        header.addView(titles);
        return header;
    }

    /**
     * The logo when the project supplies one (`[mobile] logo`), otherwise the
     * app's first letter on a secondary-coloured badge. The drawable is looked
     * up by name so a project without a logo still compiles.
     */
    private View buildMark() {
        int logo = getResources().getIdentifier("nxl_logo", "drawable", getPackageName());
        if (logo != 0) {
            ImageView image = new ImageView(this);
            image.setImageResource(logo);
            image.setScaleType(ImageView.ScaleType.CENTER_INSIDE);
            image.setPadding(dp(4), dp(4), dp(4), dp(4));
            GradientDrawable shape = new GradientDrawable();
            shape.setColor(secondary);
            shape.setCornerRadius(dp(10));
            image.setBackground(shape);
            return image;
        }

        TextView mark = new TextView(this);
        String name = getString(R.string.nxl_app_name);
        mark.setText(name.isEmpty() ? "N" : name.substring(0, 1).toUpperCase());
        mark.setTextSize(15);
        mark.setTypeface(Typeface.DEFAULT_BOLD);
        mark.setGravity(Gravity.CENTER);
        mark.setTextColor(onColor(secondary));
        GradientDrawable badge = new GradientDrawable();
        badge.setColor(secondary);
        badge.setCornerRadius(dp(10));
        mark.setBackground(badge);
        return mark;
    }

    private View buildTabBar() {
        tabBar = new LinearLayout(this);
        tabBar.setOrientation(LinearLayout.HORIZONTAL);
        tabBar.setBackgroundColor(surface);
        tabBar.setPadding(dp(12), dp(8), dp(12), dp(10));

        for (int index = 0; index < screenLabels.length; index++) {
            final int target = index;
            Button tab = new Button(this);
            tab.setText(screenLabels[index]);
            tab.setAllCaps(false);
            tab.setTextSize(13);
            tab.setTextColor(textSecondary);
            tab.setBackground(plainBackground());
            tab.setPadding(dp(8), dp(10), dp(8), dp(10));
            tab.setOnClickListener(new View.OnClickListener() {
                @Override
                public void onClick(View view) {
                    showScreen(target);
                }
            });
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                    0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
            params.setMargins(dp(4), 0, dp(4), 0);
            tabBar.addView(tab, params);
        }
        return tabBar;
    }

    // ── Screens ───────────────────────────────────────────────────────────────

    /** Compile and run the tab's program, then draw what it printed. */
    private void showScreen(int index) {
        if (screenLabels.length == 0) {
            content.addView(errorCard("No screens are configured. Add a [[mobile.screens]] "
                    + "entry to nxl.toml and rebuild."));
            return;
        }
        index = Math.max(0, Math.min(index, screenLabels.length - 1));
        content.removeAllViews();
        highlightTab(index);

        String program = screenAssets[index];
        String source = readAsset(program);
        if (source == null) {
            content.addView(errorCard("Could not read assets/" + program));
            return;
        }
        if (!NxlRuntime.isAvailable()) {
            content.addView(errorCard("libnxl_android.so did not load, so there is no NXL engine "
                    + "to run this program with."));
            return;
        }

        String whole = prelude.isEmpty()
                ? source
                : prelude + (prelude.endsWith("\n") ? "" : "\n") + source;
        String output = NxlRuntime.run(program, whole);
        if (output.startsWith("error[NXL")) {
            content.addView(errorCard(output));
            return;
        }
        render(output);
    }

    /**
     * Draw the program's output.
     *
     * <p>The markers are a convention the NXL programs use, and nothing more:
     * {@code # } is a heading, {@code - } is a bullet, a blank line is a
     * paragraph break, anything else is body text. Interpreting them is the
     * platform's job; producing the lines is NXL's.
     */
    private void render(String output) {
        for (String raw : output.split("\n", -1)) {
            String line = raw.trim();
            if (line.isEmpty()) {
                continue;
            }
            if (line.startsWith("# ")) {
                content.addView(heading(line.substring(2)));
            } else if (line.startsWith("- ")) {
                content.addView(bullet(line.substring(2)));
            } else {
                content.addView(body(line));
            }
        }
    }

    private View heading(String text) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextSize(19);
        view.setTypeface(Typeface.DEFAULT_BOLD);
        view.setTextColor(textPrimary);
        view.setPadding(0, dp(18), 0, dp(8));
        return view;
    }

    private View body(String text) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextSize(14);
        view.setLineSpacing(dp(4), 1f);
        view.setTextColor(textSecondary);
        view.setPadding(0, dp(4), 0, dp(4));
        return view;
    }

    private View bullet(String text) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setPadding(0, dp(5), 0, dp(5));

        View dot = new View(this);
        GradientDrawable shape = new GradientDrawable();
        shape.setColor(accent);
        shape.setShape(GradientDrawable.OVAL);
        row.addView(dot, new LinearLayout.LayoutParams(dp(6), dp(6)));
        ((LinearLayout.LayoutParams) dot.getLayoutParams()).setMargins(0, dp(7), dp(12), 0);

        TextView view = new TextView(this);
        view.setText(text);
        view.setTextSize(14);
        view.setLineSpacing(dp(3), 1f);
        view.setTextColor(textPrimary);
        row.addView(view, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        GradientDrawable backgroundShape = new GradientDrawable();
        backgroundShape.setColor(surface);
        backgroundShape.setCornerRadius(dp(12));
        backgroundShape.setStroke(dp(1), border);
        card.setBackground(backgroundShape);
        card.setPadding(dp(14), dp(4), dp(14), dp(4));
        card.addView(row);
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        params.setMargins(0, dp(4), 0, dp(4));
        card.setLayoutParams(params);
        return card;
    }

    private View errorCard(String message) {
        TextView view = new TextView(this);
        view.setText(message);
        view.setTextSize(13);
        view.setTextColor(0xFFFFB4B4);
        view.setLineSpacing(dp(4), 1f);
        GradientDrawable backgroundShape = new GradientDrawable();
        backgroundShape.setColor(0xFF3D1418);
        backgroundShape.setCornerRadius(dp(12));
        backgroundShape.setStroke(dp(1), 0xFF8B2C35);
        view.setBackground(backgroundShape);
        view.setPadding(dp(14), dp(12), dp(14), dp(12));
        return view;
    }

    private void highlightTab(int index) {
        for (int i = 0; i < tabBar.getChildCount(); i++) {
            Button tab = (Button) tabBar.getChildAt(i);
            boolean active = i == index;
            tab.setTextColor(active ? onColor(accent) : textSecondary);
            tab.setTypeface(active ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);
            GradientDrawable backgroundShape = new GradientDrawable();
            backgroundShape.setColor(active ? accent : Color.TRANSPARENT);
            backgroundShape.setCornerRadius(dp(10));
            tab.setBackground(backgroundShape);
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /** Read a colour resource on both API 21-22 and API 23+. */
    @SuppressWarnings("deprecation")
    private int color(int id) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            return getColor(id);
        }
        return getResources().getColor(id);
    }

    /** Black or white, whichever stays readable on {@code background}. */
    private static int onColor(int background) {
        double luminance = (0.299 * Color.red(background)
                + 0.587 * Color.green(background)
                + 0.114 * Color.blue(background)) / 255.0;
        return luminance > 0.6 ? 0xFF0B0E14 : 0xFFFFFFFF;
    }

    private String readAsset(String name) {
        try (InputStream stream = getAssets().open(name)) {
            ByteArrayOutputStream buffer = new ByteArrayOutputStream();
            byte[] chunk = new byte[4096];
            int read;
            while ((read = stream.read(chunk)) != -1) {
                buffer.write(chunk, 0, read);
            }
            return new String(buffer.toByteArray(), StandardCharsets.UTF_8);
        } catch (IOException error) {
            return null;
        }
    }

    private GradientDrawable plainBackground() {
        GradientDrawable backgroundShape = new GradientDrawable();
        backgroundShape.setColor(Color.TRANSPARENT);
        backgroundShape.setCornerRadius(dp(10));
        return backgroundShape;
    }

    private int dp(int value) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value,
                getResources().getDisplayMetrics());
    }
}
