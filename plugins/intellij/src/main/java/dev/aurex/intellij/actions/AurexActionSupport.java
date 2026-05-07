package dev.aurex.intellij.actions;

import com.intellij.notification.NotificationType;
import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.actionSystem.CommonDataKeys;
import com.intellij.openapi.fileEditor.FileEditorManager;
import com.intellij.openapi.project.DumbAwareAction;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.LocalFileSystem;
import com.intellij.openapi.vfs.VirtualFile;
import dev.aurex.intellij.AurexProjectLocator;
import dev.aurex.intellij.AurexRunner;
import java.nio.file.Path;
import java.util.List;
import java.util.Optional;

abstract class AurexActionSupport extends DumbAwareAction {
    protected Optional<Path> existingAurexRoot(AnActionEvent event) {
        Project project = event.getProject();
        if (project == null) {
            return Optional.empty();
        }

        return selectedPath(event)
                .or(() -> projectBasePath(project))
                .flatMap(start -> AurexProjectLocator.nearestRoot(
                        start,
                        projectBasePath(project).map(List::of).orElse(List.of())
                ));
    }

    protected Optional<Path> initRoot(AnActionEvent event) {
        Project project = event.getProject();
        if (project == null) {
            return Optional.empty();
        }

        return selectedDirectory(event).or(() -> projectBasePath(project));
    }

    protected void openManifest(Project project, Path root) {
        VirtualFile file = LocalFileSystem.getInstance()
                .refreshAndFindFileByNioFile(root.resolve(AurexProjectLocator.AX_TOML));
        if (file != null) {
            FileEditorManager.getInstance(project).openFile(file, true);
        }
    }

    protected void warnNoProject(Project project) {
        AurexRunner.notify(
                project,
                "No Aurex project found",
                "Could not find ax.toml from the selected file or project root.",
                NotificationType.WARNING
        );
    }

    private Optional<Path> selectedPath(AnActionEvent event) {
        VirtualFile file = event.getData(CommonDataKeys.VIRTUAL_FILE);
        if (file == null) {
            return Optional.empty();
        }
        return Optional.of(Path.of(file.getPath()));
    }

    private Optional<Path> selectedDirectory(AnActionEvent event) {
        VirtualFile file = event.getData(CommonDataKeys.VIRTUAL_FILE);
        if (file == null || !file.isDirectory()) {
            return Optional.empty();
        }
        return Optional.of(Path.of(file.getPath()));
    }

    private Optional<Path> projectBasePath(Project project) {
        String basePath = project.getBasePath();
        return basePath == null ? Optional.empty() : Optional.of(Path.of(basePath));
    }
}
