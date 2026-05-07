package dev.geralt.intellij.actions;

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import java.nio.file.Path;
import org.jetbrains.annotations.NotNull;

public final class OpenGeraltTomlAction extends GeraltActionSupport {
    @Override
    public void actionPerformed(@NotNull AnActionEvent event) {
        Project project = event.getProject();
        if (project == null) {
            return;
        }

        Path root = existingGeraltRoot(event).orElse(null);
        if (root == null) {
            warnNoProject(project);
            return;
        }

        openManifest(project, root);
    }
}
