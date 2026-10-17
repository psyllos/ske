package example;

public final class ReflectionUnknown {
    public static Class<?> load(String className) throws ClassNotFoundException {
        return Class.forName(className);
    }
}
