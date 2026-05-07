const fs = require("fs");
const path = require("path");
const vscode = require("vscode");
const {
  GERALT_TOML,
  createTaskDefinition,
  discoverGeraltProjects,
  findNearestGeraltProject,
  isGeraltProject,
  normalizeCommand,
  toDirectory,
} = require("./geraltProjects");

function activate(context) {
  const provider = new GeraltTaskProvider();

  context.subscriptions.push(
    vscode.tasks.registerTaskProvider("geralt", provider),
    vscode.commands.registerCommand("geralt.initProject", (uri) => initProject(uri)),
    vscode.commands.registerCommand("geralt.build", (uri) => runProjectCommand("build", uri)),
    vscode.commands.registerCommand("geralt.run", (uri) => runProjectCommand("run", uri)),
    vscode.commands.registerCommand("geralt.openManifest", (uri) => openManifest(uri))
  );
}

function deactivate() {}

async function initProject(uri) {
  const root = await resolveInitRoot(uri);
  if (!root) {
    return;
  }

  if (isGeraltProject(root)) {
    vscode.window.showWarningMessage(`${GERALT_TOML} already exists in ${root}`);
    return;
  }

  await executeGeraltTask("init", root);
}

async function runProjectCommand(command, uri) {
  const root = await resolveGeraltRoot(uri);
  if (root) {
    await executeGeraltTask(command, root);
  }
}

async function openManifest(uri) {
  const root = await resolveGeraltRoot(uri);
  if (!root) {
    return;
  }

  const document = await vscode.workspace.openTextDocument(path.join(root, GERALT_TOML));
  await vscode.window.showTextDocument(document);
}

async function resolveGeraltRoot(uri) {
  const folders = workspaceFolderPaths();
  const selected = selectedPath(uri);
  const nearest = findNearestGeraltProject(selected, folders);
  if (nearest) {
    return nearest;
  }

  const discovered = discoverGeraltProjects(folders);
  if (discovered.length === 1) {
    return discovered[0];
  }
  if (discovered.length > 1) {
    return pickProject(discovered);
  }

  vscode.window.showWarningMessage(`No ${GERALT_TOML} found in this workspace.`);
  return undefined;
}

async function resolveInitRoot(uri) {
  if (uri?.fsPath) {
    return toDirectory(uri.fsPath);
  }

  const folders = workspaceFolderPaths();
  if (folders.length === 1) {
    return folders[0];
  }
  if (folders.length > 1) {
    return pickProject(folders);
  }

  const picked = await vscode.window.showOpenDialog({
    canSelectFiles: false,
    canSelectFolders: true,
    canSelectMany: false,
    title: "Choose a folder for geralt init",
  });
  return picked?.[0]?.fsPath;
}

async function pickProject(projects) {
  const picked = await vscode.window.showQuickPick(
    projects.map((project) => ({ label: path.basename(project), description: project, project })),
    { placeHolder: "Choose a Geralt project" }
  );
  return picked?.project;
}

async function executeGeraltTask(command, cwd) {
  normalizeCommand(command);
  const config = vscode.workspace.getConfiguration("geralt");
  const executable = config.get("executablePath", "geralt");
  const reveal = config.get("revealTerminal", true);
  const execution = new vscode.ShellExecution(executable, [command], { cwd });
  const task = new vscode.Task(
    createTaskDefinition(command, cwd, executable),
    vscode.TaskScope.Workspace,
    `Geralt: ${command}`,
    "geralt",
    execution
  );

  task.presentationOptions = {
    reveal: reveal ? vscode.TaskRevealKind.Always : vscode.TaskRevealKind.Silent,
    panel: vscode.TaskPanelKind.Dedicated,
    clear: true,
  };

  return vscode.tasks.executeTask(task);
}

function selectedPath(uri) {
  if (uri?.fsPath) {
    return uri.fsPath;
  }

  const active = vscode.window.activeTextEditor?.document?.uri;
  if (active?.scheme === "file") {
    return active.fsPath;
  }

  return workspaceFolderPaths()[0];
}

function workspaceFolderPaths() {
  return (vscode.workspace.workspaceFolders ?? []).map((folder) => folder.uri.fsPath);
}

class GeraltTaskProvider {
  provideTasks() {
    return discoverGeraltProjects(workspaceFolderPaths()).flatMap((root) => [
      this.createTask("build", root),
      this.createTask("run", root),
    ]);
  }

  resolveTask(task) {
    const command = task.definition.command;
    const cwd = task.definition.cwd;
    if (!command || !cwd || !fs.existsSync(path.join(cwd, GERALT_TOML))) {
      return undefined;
    }
    return this.createTask(command, cwd);
  }

  createTask(command, cwd) {
    return new vscode.Task(
      createTaskDefinition(command, cwd, executablePath()),
      vscode.TaskScope.Workspace,
      `Geralt: ${command} (${path.basename(cwd)})`,
      "geralt",
      new vscode.ShellExecution(executablePath(), [command], { cwd })
    );
  }
}

function executablePath() {
  return vscode.workspace.getConfiguration("geralt").get("executablePath", "geralt");
}

module.exports = {
  activate,
  deactivate,
};
