# Aurex for IntelliJ

This plugin adds Tools menu actions for Aurex Java projects:

- Init Aurex Project
- Build Aurex Project
- Run Aurex Project
- Open aurex.toml

The runner uses `AUREX_EXECUTABLE` when set, otherwise it runs `ax` from `PATH`.

## Local Tests

The project locator and command validation tests can run without Gradle or the IntelliJ SDK:

```powershell
.\scripts\test.ps1
```

The full plugin build uses the IntelliJ Platform Gradle Plugin:

```powershell
gradle testProjectLocator buildPlugin verifyPlugin
```

Use Gradle 9 or newer with Java 17 or newer.
