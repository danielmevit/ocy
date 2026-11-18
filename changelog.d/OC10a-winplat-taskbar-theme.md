# 0.1.2 — Windows taskbar theme

The Windows platform layer now implements the 0.1.2 contract `ITaskbarThemeProvider`
(`IPlatformServices.TaskbarTheme`). It reads the `SystemUsesLightTheme` DWORD under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` and reports a light or dark
taskbar so the tray icon can match it, independently of the app theme. A missing value (Windows 10
before 1903) or any error reports unknown (`null`) and never throws.
