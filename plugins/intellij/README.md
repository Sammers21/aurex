# Geralt for IntelliJ

This plugin adds Tools menu actions for Geralt Java projects:

- Init Geralt Project
- Build Geralt Project
- Run Geralt Project
- Open geralt.toml

The runner uses `GERALT_EXECUTABLE` when set, otherwise it runs `geralt` from `PATH`.

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
