const fs = require("fs");
const path = require("path");
const vscode = require("vscode");
const {
  AX_TOML,
  createTaskDefinition,
  discoverAurexProjects,
  findNearestAurexProject,
  isAurexProject,
  normalizeCommand,
  toDirectory,
} = require("./aurexProjects");

function activate(context) {
  const provider = new AurexTaskProvider();

  context.subscriptions.push(
    vscode.tasks.registerTaskProvider("aurex", provider),
    vscode.commands.registerCommand("aurex.initProject", (uri) => initProject(uri)),
    vscode.commands.registerCommand("aurex.build", (uri) => runProjectCommand("build", uri)),
    vscode.commands.registerCommand("aurex.run", (uri) => runProjectCommand("run", uri)),
    vscode.commands.registerCommand("aurex.openManifest", (uri) => openManifest(uri))
  );
}

function deactivate() {}

async function initProject(uri) {
  const root = await resolveInitRoot(uri);
  if (!root) {
    return;
  }

  if (isAurexProject(root)) {
    vscode.window.showWarningMessage(`${AX_TOML} already exists in ${root}`);
    return;
  }

  await executeAurexTask("init", root);
}

async function runProjectCommand(command, uri) {
  const root = await resolveAurexRoot(uri);
  if (root) {
    await executeAurexTask(command, root);
  }
}

async function openManifest(uri) {
  const root = await resolveAurexRoot(uri);
  if (!root) {
    return;
  }

  const document = await vscode.workspace.openTextDocument(path.join(root, AX_TOML));
  await vscode.window.showTextDocument(document);
}

async function resolveAurexRoot(uri) {
  const folders = workspaceFolderPaths();
  const selected = selectedPath(uri);
  const nearest = findNearestAurexProject(selected, folders);
  if (nearest) {
    return nearest;
  }

  const discovered = discoverAurexProjects(folders);
  if (discovered.length === 1) {
    return discovered[0];
  }
  if (discovered.length > 1) {
    return pickProject(discovered);
  }

  vscode.window.showWarningMessage(`No ${AX_TOML} found in this workspace.`);
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
    title: "Choose a folder for ax init",
  });
  return picked?.[0]?.fsPath;
}

async function pickProject(projects) {
  const picked = await vscode.window.showQuickPick(
    projects.map((project) => ({ label: path.basename(project), description: project, project })),
    { placeHolder: "Choose an Aurex project" }
  );
  return picked?.project;
}

async function executeAurexTask(command, cwd) {
  normalizeCommand(command);
  const config = vscode.workspace.getConfiguration("aurex");
  const executable = config.get("executablePath", "ax");
  const reveal = config.get("revealTerminal", true);
  const execution = new vscode.ShellExecution(executable, [command], { cwd });
  const task = new vscode.Task(
    createTaskDefinition(command, cwd, executable),
    vscode.TaskScope.Workspace,
    `Aurex: ${command}`,
    "aurex",
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

class AurexTaskProvider {
  provideTasks() {
    return discoverAurexProjects(workspaceFolderPaths()).flatMap((root) => [
      this.createTask("build", root),
      this.createTask("run", root),
    ]);
  }

  resolveTask(task) {
    const command = task.definition.command;
    const cwd = task.definition.cwd;
    if (!command || !cwd || !fs.existsSync(path.join(cwd, AX_TOML))) {
      return undefined;
    }
    return this.createTask(command, cwd);
  }

  createTask(command, cwd) {
    return new vscode.Task(
      createTaskDefinition(command, cwd, executablePath()),
      vscode.TaskScope.Workspace,
      `Aurex: ${command} (${path.basename(cwd)})`,
      "aurex",
      new vscode.ShellExecution(executablePath(), [command], { cwd })
    );
  }
}

function executablePath() {
  return vscode.workspace.getConfiguration("aurex").get("executablePath", "ax");
}

module.exports = {
  activate,
  deactivate,
};
