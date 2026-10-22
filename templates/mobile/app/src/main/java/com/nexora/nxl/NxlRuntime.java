package com.nexora.nxl;

/**
 * The bridge to the NXL engine.
 *
 * <p>The native library behind this class is the NXL compiler and the NXL
 * interpreter — the same lexer, parser, analyser and virtual machine the
 * desktop compiler runs, compiled for Android. Nothing here interprets NXL; it
 * only hands source text across JNI and takes back what the program printed.
 *
 * <p>Both methods are static because the native side expects a {@code jclass}
 * as its second argument. The names below are bound to the symbols
 * {@code Java_com_nexora_nxl_NxlRuntime_run} and
 * {@code Java_com_nexora_nxl_NxlRuntime_check}.
 */
public final class NxlRuntime {

    private static boolean loaded;

    static {
        try {
            System.loadLibrary("nxl_android");
            loaded = true;
        } catch (UnsatisfiedLinkError error) {
            loaded = false;
        }
    }

    private NxlRuntime() {
    }

    /** Whether the NXL library loaded. Checked before every run. */
    public static boolean isAvailable() {
        return loaded;
    }

    /**
     * Compile and run a NXL program.
     *
     * @param name   file name used in diagnostics, e.g. {@code welcome.nxl}
     * @param source the program text
     * @return everything the program printed, or a diagnostic beginning
     *         {@code error[NXL…]:} when it did not compile or run
     */
    public static native String run(String name, String source);

    /**
     * Type-check a NXL program without running it.
     *
     * @return {@code "ok"}, or a diagnostic
     */
    public static native String check(String name, String source);
}
