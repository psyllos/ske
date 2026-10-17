package example;

public final class Decide {
    public static int decide(int x) {
        if (x < 0) {
            return -1;
        }
        if (x < -10) {
            return -2;
        }
        return 0;
    }

    public static String day(int n) {
        switch (n) {
            case 1:
                return "one";
            case 2:
            case 3:
                return "few";
            default:
                return "many";
        }
    }

    public static int guarded(Runnable r) {
        try {
            r.run();
            return 0;
        } catch (RuntimeException e) {
            return -1;
        }
    }
}
