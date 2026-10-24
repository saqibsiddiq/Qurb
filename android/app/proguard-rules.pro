# JNA and the generated bindings are reached reflectively, so nothing that
# walks the call graph can see they are used.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }

# JNA has a path for desktop Java that reaches for AWT window handles. Android
# has no AWT and never takes that path, but R8 sees the references and refuses
# to finish. Nothing here is kept; this only says the absence is expected.
-dontwarn java.awt.**

# The theme names the view inflater (viewInflaterClass) and AppCompat loads it
# by that name, which R8 cannot see.
-keep class com.qurb.QurbViewInflater { <init>(); }
