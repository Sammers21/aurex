const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const {
  createTaskDefinition,
  discoverAurexProjects,
  findNearestAurexProject,
  normalizeCommand,
  toDirectory,
} = require("../src/aurexProjects");

test("findNearestAurexProject resolves from nested source files", () => {
  const root = tempDir();
  const project = path.join(root, "service");
  const source = path.join(project, "src", "com", "example", "Main.java");
  fs.mkdirSync(path.dirname(source), { recursive: true });
  fs.writeFileSync(path.join(project, "ax.toml"), "[package]\nname = \"service\"\n");
  fs.writeFileSync(source, "class Main {}\n");

  assert.equal(findNearestAurexProject(source, [root]), project);
});

test("findNearestAurexProject stops at workspace boundary", () => {
  const root = tempDir();
  const outside = path.join(root, "ax.toml");
  const workspace = path.join(root, "workspace");
  const nested = path.join(workspace, "src", "Main.java");
  fs.mkdirSync(path.dirname(nested), { recursive: true });
  fs.writeFileSync(outside, "[package]\nname = \"outside\"\n");
  fs.writeFileSync(nested, "class Main {}\n");

  assert.equal(findNearestAurexProject(nested, [workspace]), undefined);
});

test("discoverAurexProjects finds nested projects and skips generated folders", () => {
  const root = tempDir();
  const api = path.join(root, "api");
  const cli = path.join(root, "tools", "cli");
  const generated = path.join(root, "target", "ignored");
  for (const directory of [api, cli, generated]) {
    fs.mkdirSync(directory, { recursive: true });
    fs.writeFileSync(path.join(directory, "ax.toml"), "[package]\nname = \"demo\"\n");
  }

  assert.deepEqual(discoverAurexProjects([root]), [api, cli].sort());
});

test("createTaskDefinition normalizes commands and cwd", () => {
  const cwd = tempDir();

  assert.deepEqual(createTaskDefinition("build", cwd, "C:/bin/ax.exe"), {
    type: "aurex",
    command: "build",
    cwd: path.resolve(cwd),
    executable: "C:/bin/ax.exe",
  });
  assert.throws(() => normalizeCommand("delete"), /Unsupported Aurex command/);
  assert.equal(normalizeCommand("test"), "test");
  assert.equal(normalizeCommand("clean"), "clean");
  assert.equal(normalizeCommand("fmt"), "fmt");
});

test("toDirectory returns parent for files and itself for directories", () => {
  const root = tempDir();
  const file = path.join(root, "ax.toml");
  fs.writeFileSync(file, "");

  assert.equal(toDirectory(file), root);
  assert.equal(toDirectory(root), root);
});

test("package manifest contributes Aurex commands and task type", () => {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8")
  );
  const commands = manifest.contributes.commands.map((command) => command.command);

  assert.deepEqual(commands, [
    "aurex.initProject",
    "aurex.build",
    "aurex.run",
    "aurex.test",
    "aurex.clean",
    "aurex.format",
    "aurex.openManifest",
  ]);
  assert.equal(manifest.contributes.taskDefinitions[0].type, "aurex");
  assert.equal(manifest.contributes.taskDefinitions[0].properties.cwd.type, "string");
  assert.equal(manifest.contributes.taskDefinitions[0].properties.executable.type, "string");
  assert.equal(manifest.contributes.configuration.properties["aurex.executablePath"].default, "ax");
});

function tempDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "aurex-vscode-"));
}
