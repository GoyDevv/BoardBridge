/*
 * Copyright 2026 The BoardBridge Authors
 * Licensed under the Apache License, Version 2.0.
 *
 * Minimal "hello JVM" program. Compiled to standard JVM bytecode (NOT dexed)
 * and run inside the bundled OpenJDK 21 via JNI_CreateJavaVM, so its output is
 * proof that a real desktop JVM is executing on the device/emulator.
 */
public class HelloJvm {
    public static void main(String[] args) {
        System.out.println("Hello from JVM!"
                + " java.version=" + System.getProperty("java.version")
                + " java.vm.name=" + System.getProperty("java.vm.name")
                + " os.arch=" + System.getProperty("os.arch")
                + " os.name=" + System.getProperty("os.name"));
        System.out.println("HelloJvm: args=" + args.length + " java.home=" + System.getProperty("java.home"));
        System.err.println("HelloJvm: stderr channel works too");
    }
}
