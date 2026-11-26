package agenttest;

/** Methods with every control-flow shape the agent marks; the self-test calls some of them. */
public final class Sample {
    private Sample() {}

    public static int pick(int x) {
        if (x > 0) {
            return 1;
        }
        return 2;
    }

    public static int sw(int x) {
        switch (x) {
            case 0:
                return 10;
            case 1:
                return 11;
            default:
                return 12;
        }
    }

    public static int parse(String s) {
        try {
            return Integer.parseInt(s);
        } catch (NumberFormatException e) {
            return -1;
        }
    }

    public static int loop(int n) {
        int sum = 0;
        for (int i = 0; i < n; i++) {
            sum += i;
        }
        return sum;
    }

    public static void unused() {}
}
