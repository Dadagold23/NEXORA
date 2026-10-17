package com.nexora.nxl;

import android.app.Activity;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Bundle;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/**
 * The NEXORA starter for Android.
 *
 * <p>What is NXL and what is Android, precisely:
 *
 * <ul>
 *   <li><b>NXL</b> produces every word on screen. Each tab holds a
 *       {@code .nxl} program in {@code assets/}; the text is passed to the NXL
 *       interpreter over JNI and what it prints is drawn.</li>
 *   <li><b>Android</b> owns the activity, the views, the scrolling, the taps,
 *       and the choice of which program a tab runs.</li>
 * </ul>
 *
 * <p>That split is the honest one. Drawing is the platform's job. The content,
 * the loops that build it and the branching that chooses it are the language's.
 */
public final class MainActivity extends Activity {

    /** The screens, in tab order. Each entry is a program in {@code assets/}. */
    private static final String[][] SCREENS = {
            {"Welcome", "welcome.nxl"},
            {"Features", "features.nxl"},
            {"About", "about.nxl"},
    };

    private static final int BACKGROUND = 0xFF0B0E14;
    private static final int SURFACE = 0xFF141A24;
    private static final int SURFACE_RAISED = 0xFF1B2432;
    private static final int BORDER = 0xFF263243;
    private static final int TEXT_PRIMARY = 0xFFE8EEF7;
    private static final int TEXT_SECONDARY = 0xFF93A3B8;
    private static final int ACCENT = 0xFF5AA9FF;

    private LinearLayout content;
    private LinearLayout tabBar;
    private int selected;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(buildLayout());
        showScreen(0);
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    private View buildLayout() {
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(BACKGROUND);

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
        header.setBackgroundColor(SURFACE);
        header.setPadding(dp(20), dp(18), dp(20), dp(18));

        TextView mark = new TextView(this);
        mark.setText("N");
        mark.setTextSize(15);
        mark.setTypeface(Typeface.DEFAULT_BOLD);
        mark.setGravity(Gravity.CENTER);
        mark.setTextColor(0xFF0B0E14);
        GradientDrawable badge = new GradientDrawable();
        badge.setColor(ACCENT);
        badge.setCornerRadius(dp(10));
        mark.setBackground(badge);
        header.addView(mark, new LinearLayout.LayoutParams(dp(34), dp(34)));

        LinearLayout titles = new LinearLayout(this);
        titles.setOrientation(LinearLayout.VERTICAL);
        titles.setPadding(dp(12), 0, 0, 0);

        TextView name = new TextView(this);
        name.setText("NEXORA");
        name.setTextSize(16);
        name.setTypeface(Typeface.DEFAULT_BOLD);
        name.setLetterSpacing(0.18f);
        name.setTextColor(TEXT_PRIMARY);
        titles.addView(name);

        TextView engine = new TextView(this);
        engine.setText(NxlRuntime.isAvailable()
                ? "NXL interpreter in-process"
                : "NXL library not loaded");
        engine.setTextSize(11);
        engine.setLetterSpacing(0.04f);
        engine.setTextColor(NxlRuntime.isAvailable() ? TEXT_SECONDARY : 0xFFFF6B6B);
        titles.addView(engine);

        header.addView(titles);
        return header;
    }

    private View buildTabBar() {
        tabBar = new LinearLayout(this);
        tabBar.setOrientation(LinearLayout.HORIZONTAL);
        tabBar.setBackgroundColor(SURFACE);
        tabBar.setPadding(dp(12), dp(8), dp(12), dp(10));

        for (int index = 0; index < SCREENS.length; index++) {
            final int target = index;
            Button tab = new Button(this);
            tab.setText(SCREENS[index][0]);
            tab.setAllCaps(false);
            tab.setTextSize(13);
            tab.setTextColor(TEXT_SECONDARY);
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
        selected = index;
        content.removeAllViews();
        highlightTab(index);

        String program = SCREENS[index][1];
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

        String output = NxlRuntime.run(program, source);
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
        view.setTextColor(TEXT_PRIMARY);
        view.setPadding(0, dp(18), 0, dp(8));
        return view;
    }

    private View body(String text) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextSize(14);
        view.setLineSpacing(dp(4), 1f);
        view.setTextColor(TEXT_SECONDARY);
        view.setPadding(0, dp(4), 0, dp(4));
        return view;
    }

    private View bullet(String text) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setPadding(0, dp(5), 0, dp(5));

        View dot = new View(this);
        GradientDrawable shape = new GradientDrawable();
        shape.setColor(ACCENT);
        shape.setShape(GradientDrawable.OVAL);
        row.addView(dot, new LinearLayout.LayoutParams(dp(6), dp(6)));
        ((LinearLayout.LayoutParams) dot.getLayoutParams()).setMargins(0, dp(7), dp(12), 0);

        TextView view = new TextView(this);
        view.setText(text);
        view.setTextSize(14);
        view.setLineSpacing(dp(3), 1f);
        view.setTextColor(TEXT_PRIMARY);
        row.addView(view, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        GradientDrawable background = new GradientDrawable();
        background.setColor(SURFACE);
        background.setCornerRadius(dp(12));
        background.setStroke(dp(1), BORDER);
        card.setBackground(background);
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
        GradientDrawable background = new GradientDrawable();
        background.setColor(0xFF3D1418);
        background.setCornerRadius(dp(12));
        background.setStroke(dp(1), 0xFF8B2C35);
        view.setBackground(background);
        view.setPadding(dp(14), dp(12), dp(14), dp(12));
        return view;
    }

    private void highlightTab(int index) {
        for (int i = 0; i < tabBar.getChildCount(); i++) {
            Button tab = (Button) tabBar.getChildAt(i);
            boolean active = i == index;
            tab.setTextColor(active ? 0xFF0B0E14 : TEXT_SECONDARY);
            tab.setTypeface(active ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);
            GradientDrawable background = new GradientDrawable();
            background.setColor(active ? ACCENT : Color.TRANSPARENT);
            background.setCornerRadius(dp(10));
            tab.setBackground(background);
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

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
        GradientDrawable background = new GradientDrawable();
        background.setColor(Color.TRANSPARENT);
        background.setCornerRadius(dp(10));
        return background;
    }

    private int dp(int value) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value,
                getResources().getDisplayMetrics());
    }
}
