package schemora.tools;

import java.lang.reflect.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public final class SnapshotRuntime {
    public static void main(String[] args) throws Exception {
        String[] generation = Arrays.copyOfRange(args, 4, args.length);
        Class.forName("net.minecraft.data.Main").getMethod("main", String[].class)
            .invoke(null, (Object) generation);
        Class<?> entityType = Class.forName(args[1]);
        Class<?> living = Class.forName(args[2]);
        Object registry = null;
        for (Field field : Class.forName(args[3]).getDeclaredFields()) {
            if (!Modifier.isStatic(field.getModifiers())) continue;
            Type type = field.getGenericType();
            if (!(type instanceof ParameterizedType)) continue;
            Type entry = ((ParameterizedType) type).getActualTypeArguments()[0];
            if (entry instanceof ParameterizedType) entry = ((ParameterizedType) entry).getRawType();
            if (entry == entityType) {
                field.setAccessible(true);
                registry = field.get(null);
                break;
            }
        }
        if (registry == null) throw new IllegalStateException("Entity registry was not found");
        Map<String, String> result = new TreeMap<>();
        Class<?> constants;
        try {
            constants = Class.forName(entityType.getName() + "s");
        } catch (ClassNotFoundException missing) {
            constants = entityType;
        }
        for (Field field : constants.getDeclaredFields()) {
            if (!Modifier.isStatic(field.getModifiers()) || field.getType() != entityType) continue;
            if (!(field.getGenericType() instanceof ParameterizedType)) continue;
            Type entry = ((ParameterizedType) field.getGenericType()).getActualTypeArguments()[0];
            if (entry instanceof WildcardType) entry = ((WildcardType) entry).getUpperBounds()[0];
            if (entry instanceof ParameterizedType) entry = ((ParameterizedType) entry).getRawType();
            if (!(entry instanceof Class<?>)) throw new IllegalStateException("Unknown entity class: " + field);
            Class<?> entity = (Class<?>) entry;
            field.setAccessible(true);
            Object value = field.get(null);
            String identifier = null;
            for (Method method : registry.getClass().getMethods()) {
                if (method.getParameterCount() != 1 || method.getParameterTypes()[0] != Object.class
                    || method.getReturnType().isPrimitive()) continue;
                Object key = method.invoke(registry, value);
                if (key != null && key.toString().matches("minecraft:[a-z0-9_./]+")) {
                    identifier = key.toString();
                    break;
                }
            }
            if (identifier == null) throw new IllegalStateException("Entity ID was not found: " + field);
            result.put(identifier, "{\"type\":\"" + (living.isAssignableFrom(entity) ? "living" : "other")
                + "\",\"runtime_class\":\"" + entity.getName() + "\"}");
        }
        List<String> entries = new ArrayList<>();
        for (Map.Entry<String, String> entry : result.entrySet()) {
            entries.add("\"" + entry.getKey() + "\":" + entry.getValue());
        }
        Files.write(Paths.get(args[0]), ("{" + String.join(",", entries) + "}").getBytes(StandardCharsets.UTF_8));
    }
}
