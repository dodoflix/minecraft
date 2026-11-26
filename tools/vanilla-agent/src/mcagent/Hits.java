package mcagent;

/**
 * Hit markers of the instrumented classes (the agent jar is on the system class path, which every loader of the program
 * delegates to). {@code F} holds one flag byte per counter (set to 1), {@code C} one int per counter (count mode).
 */
public final class Hits {
    public static byte[] F = new byte[0];
    public static int[] C = new int[0];

    private Hits() {}
}
