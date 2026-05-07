# Aurex

Aurex is a Cargo-like build system for Java projects. It compiles Java sources
with `javac`, resolves Maven dependencies, and packages runnable classpath or
fat jars from a small `ax.toml` file.

IDE helpers for VS Code and IntelliJ live under `plugins/`.

## Hello World

You can initialize a new Aurex project by running:

```bash
$ ax init
```

This will create a new Aurex project in the current directory. The structure of the project will look like this:

```
src/
  com/
    example/
      Main.java
ax.toml
```

The `Main.java` file will contain the following code:

```java
package com.example;

public class Main {
    public static void main(String[] args) {
        System.out.println("Hello, world!");
    }
}
```

and the `ax.toml` file will contain the following configuration:

```toml
[package]
name = "hello-world"
version = "0.0.1"

[dependencies]
```

You can build the project by running:

```bash
$ ax run
```

This will compile the project and run the `Main` class.

You can also build the project without running it by running:

```bash
$ ax build
```

This will compile the project, resolve dependencies into `target/deps`, and create the configured runnable jar.

To check which Java runtime Aurex will use from your current shell, run:

```bash
$ ax java
```

Aurex uses the `java`, `javac`, and `jar` commands available on your shell
`PATH`; `JAVA_HOME` is not used for tool selection.

In order to add dependencies to your project, you can add them to the `ax.toml` file under the `[dependencies]` section. For example, to add the `org.apache.commons:commons-lang3:3.12.0` dependency, you can add the following line:

```toml
[dependencies]
"org.apache.commons:commons-lang3" = "3.12.0"
```

You can then run `ax build` to download the dependency and build the project.

Dependencies are resolved from configured Maven repositories first, then Maven
Central. You can add repositories with optional basic auth:

```toml
[[repositories]]
name = "internal"
url = "https://repo.example.com/maven2"
username = "user"
password = "pass"
```

By default Aurex creates a classpath jar whose manifest points at dependency
jars in `target/deps`. To build one merged jar instead, set:

```toml
[build]
jar_mode = "fat"
```

To package non-Java resources into the jar, add resource roots. Files are copied
relative to each configured directory:

```toml
[resources]
dirs = ["settings"]
```

## Examples

The `examples/` folder contains runnable Aurex subprojects that exercise
different project shapes:

- `basic`: no-dependency hello world project.
- `vertx`: async framework example with transitive Maven dependencies.
- `text-utils`: text processing with Apache Commons Text.
- `json-report`: multi-class JSON serialization built as a fat jar.
- `cli-orders`: Picocli command-style app built as a fat jar.

Run them through the integration tests with:

```bash
cargo test --test examples
```

## IDE Plugins

Aurex IDE helpers live under `plugins/`:

- `plugins/vscode`: VS Code extension with init/build/run/open commands,
  task provider support, settings, and `ax.toml` snippets.
- `plugins/intellij`: IntelliJ Platform plugin project with Tools menu and
  project-view actions for init/build/run/open.

Plugin-local tests can be run with:

```bash
cd plugins/vscode && npm test
cd plugins/intellij && ./scripts/test.ps1
```

## Installation

You can install Aurex by running:

MacOS with [Homebrew](https://brew.sh/):

```bash
brew install aurex
ax init
```

Linux via [sdkman](https://sdkman.io/):

```bash
sdk install aurex
ax init
```
