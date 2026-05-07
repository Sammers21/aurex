# Aurex (ax)

Aurex (`ax`) is a small build tool for Java applications. It reads `ax.toml`,
compiles Java sources with `javac`, resolves Maven dependencies, copies
configured resources, and writes a runnable jar.

## Quick Start

Create a project in the current directory:

```bash
ax init
```

This creates `src/com/example/Main.java` and `ax.toml`. Build and run the
generated project:

```bash
ax run
```

Build without running:

```bash
ax build
```

Add or remove Maven dependencies:

```bash
ax add org.apache.commons:commons-lang3@3.14.0
ax remove org.apache.commons:commons-lang3
```

Run JUnit 5 tests:

```bash
ax test
ax t
```

Format Java sources:

```bash
ax fmt
```

Remove generated output:

```bash
ax clean
```

Print the Java runtime that `ax` resolves from the current shell:

```bash
ax java
```

`ax build` uses `javac` from `PATH`; `ax run` builds first, then runs the jar
with `java -jar`.

## ax.toml

```toml
[package]
name = "my-app"
version = "0.1.0"
jar_name = "my-app.jar"
root = "./src"
test_root = "./src/test/java"
main = "com.example.Main"

[[repositories]]
name = "internal"
url = "https://repo.example.com/maven2"
username = "user"
password = "pass"

[build]
jar_mode = "fat"

[resources]
dirs = ["settings"]

[dependencies]
"org.apache.commons:commons-lang3" = "3.14.0"
```

`[package]` is required. `name` is required; `version` defaults to `0.0.1`;
`jar_name` defaults to `<name>-<version>.jar`; `root` defaults to `./src`;
`test_root` defaults to `./src/test/java`; `main` defaults to
`com.example.Main`. `main` is a fully qualified Java class name, not a source
file path.

`[dependencies]` maps `"groupId:artifactId"` to a release version. Aurex
downloads root and transitive jars into `target/deps`; `SNAPSHOT` versions are
rejected.

`[[repositories]]` entries are tried before Maven Central. `username` and
`password` are optional, but must be configured together when basic auth is
needed.

`[build].jar_mode` can be `classpath` or `fat`. Classpath mode is the default
and writes a manifest `Class-Path` pointing at jars in `target/deps`. Fat mode
creates one merged jar containing project classes, resources, and dependency
jar contents.

`[resources].dirs` lists directories to copy into the compiled classes before
packaging. Files are packaged relative to each configured directory.

`ax test` compiles production sources, compiles tests into
`target/test-classes`, and runs JUnit 5 with
`junit-platform-console-standalone`.

`ax fmt` formats `.java` files under `root` and `test_root`. If
`eclipse-formatter.xml` is present in the project root, Aurex uses Eclipse JDT;
otherwise it uses google-java-format. Managed test and formatter jars are kept
under `target/tools`.

## Examples

The `examples/` directory contains runnable `ax` projects:

- `basic`: no-dependency hello world project.
- `vertx`: Vert.x app with transitive Maven dependencies.
- `text-utils`: Apache Commons Text example.
- `json-report`: Gson example built as a fat jar.
- `cli-orders`: Picocli command-style app built as a fat jar.

Run the example integration tests with:

```bash
cargo test --test examples
```

## IDE Helpers

IDE helper projects live under `plugins/`:

- `plugins/vscode`: VS Code commands, task provider, settings, and `ax.toml`
  snippets.
- `plugins/intellij`: IntelliJ actions for init, build, run, test, clean, fmt, and opening
  `ax.toml`.

Plugin-local tests:

```bash
cd plugins/vscode && npm test
cd plugins/intellij && ./scripts/test.ps1
```
