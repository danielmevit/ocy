# 0.1.2 — Windows "Animation effects" preference

The Windows platform layer now implements the 0.1.2 contract `IMotionPreference`
(`IPlatformServices.Motion`). It reads the OS "Animation effects" setting via
`user32.dll!SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)` and reports
`PrefersReducedMotion() == true` when the user turned animations off, so the app can show still
illustrations instead of animating them. When the call fails or the setting allows animations, it
reports `false` (animations allowed) and never throws.
