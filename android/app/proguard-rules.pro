# JNA and the generated bindings are reached reflectively, so nothing that
# walks the call graph can see they are used.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
