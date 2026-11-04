package extractor;

import java.lang.reflect.Field;
import java.lang.reflect.Method;

/** Reflection helpers for vanilla's private/protected state (the extractor reads it exactly as vanilla stores it). */
public final class R {
   private R() {
   }

   public static Field field(Class<?> c, String name) throws NoSuchFieldException {
      for (Class<?> k = c; k != null; k = k.getSuperclass()) {
         try {
            Field f = k.getDeclaredField(name);
            f.setAccessible(true);
            return f;
         } catch (NoSuchFieldException ignored) {
         }
      }
      throw new NoSuchFieldException(c.getName() + "." + name);
   }

   @SuppressWarnings("unchecked")
   public static <T> T get(Object o, String name) {
      try {
         return (T)field(o.getClass(), name).get(o);
      } catch (ReflectiveOperationException e) {
         throw new RuntimeException(e);
      }
   }

   @SuppressWarnings("unchecked")
   public static <T> T getStatic(Class<?> c, String name) {
      try {
         return (T)field(c, name).get(null);
      } catch (ReflectiveOperationException e) {
         throw new RuntimeException(e);
      }
   }

   public static Object call(Object o, String name, Object... args) {
      for (Class<?> k = o.getClass(); k != null; k = k.getSuperclass()) {
         for (Method m : k.getDeclaredMethods()) {
            if (m.getName().equals(name) && m.getParameterCount() == args.length) {
               try {
                  m.setAccessible(true);
                  return m.invoke(o, args);
               } catch (ReflectiveOperationException e) {
                  throw new RuntimeException(e);
               }
            }
         }
      }
      throw new RuntimeException("no method " + name + " on " + o.getClass());
   }

   /** Simple class name, skipping anonymous classes. */
   public static String simpleName(Class<?> c) {
      while (c != null && c.getSimpleName().isEmpty()) {
         c = c.getSuperclass();
      }
      return c == null ? "" : c.getSimpleName();
   }
}
