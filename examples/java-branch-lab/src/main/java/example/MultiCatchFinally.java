package example;

public final class MultiCatchFinally {
    public static int guarded(String input) {
        try {
            return Integer.parseInt(input);
        } catch (NumberFormatException ex) {
            return -1;
        } catch (RuntimeException ex) {
            return -2;
        } finally {
            System.out.println("done");
        }
    }
}
