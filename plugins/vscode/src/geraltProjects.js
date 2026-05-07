const fs = require("fs");
const path = require("path");

const GERALT_TOML = "geralt.toml";
const VALID_COMMANDS = new Set(["init", "build", "run"]);
const IGNORED_DIRS = new Set([".git", ".idea", "build", "node_modules", "out", "target"]);

function normalizeCommand(command) {
  if (!VALID_COMMANDS.has(command)) {
    throw new Error(`Unsupported Geralt command: ${command}`);
  }
  return command;
}

function isGeraltProject(directory, fsImpl = fs) {
  return fileExists(path.join(directory, GERALT_TOML), fsImpl);
}

function findNearestGeraltProject(startPath, workspaceFolders = [], fsImpl = fs) {
  const start = toDirectory(startPath, fsImpl);
  if (!start) {
    return undefined;
  }

  const boundaries = workspaceFolders.map((folder) => path.resolve(folder));
  for (let current = path.resolve(start); ; current = path.dirname(current)) {
    if (isGeraltProject(current, fsImpl)) {
      return current;
    }

    if (boundaries.includes(current) || path.dirname(current) === current) {
      return undefined;
    }
  }
}

function discoverGeraltProjects(workspaceFolders, fsImpl = fs) {
  const projects = [];
  for (const folder of workspaceFolders) {
    walk(path.resolve(folder), fsImpl, projects);
  }
  return [...new Set(projects)].sort();
}

function createTaskDefinition(command, cwd, executable = "geralt") {
  return {
    type: "geralt",
    command: normalizeCommand(command),
    cwd: path.resolve(cwd),
    executable,
  };
}

function toDirectory(candidate, fsImpl = fs) {
  if (!candidate) {
    return undefined;
  }

  const resolved = path.resolve(candidate);
  try {
    return fsImpl.statSync(resolved).isDirectory() ? resolved : path.dirname(resolved);
  } catch {
    return path.extname(resolved) ? path.dirname(resolved) : resolved;
  }
}

function walk(directory, fsImpl, projects) {
  let entries;
  try {
    if (!fsImpl.statSync(directory).isDirectory()) {
      return;
    }
    if (isGeraltProject(directory, fsImpl)) {
      projects.push(directory);
    }
    entries = fsImpl.readdirSync(directory, { withFileTypes: true });
  } catch {
    return;
  }

  for (const entry of entries) {
    if (!entry.isDirectory() || IGNORED_DIRS.has(entry.name)) {
      continue;
    }
    walk(path.join(directory, entry.name), fsImpl, projects);
  }
}

function fileExists(file, fsImpl) {
  try {
    return fsImpl.statSync(file).isFile();
  } catch {
    return false;
  }
}

module.exports = {
  GERALT_TOML,
  createTaskDefinition,
  discoverGeraltProjects,
  findNearestGeraltProject,
  isGeraltProject,
  normalizeCommand,
  toDirectory,
};
