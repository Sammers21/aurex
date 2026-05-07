package dev.geralt.intellij.actions;

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import dev.geralt.intellij.GeraltCommand;
import dev.geralt.intellij.GeraltRunner;
import java.nio.file.Path;
import org.jetbrains.annotations.NotNull;

public final class BuildGeraltProjectAction extends GeraltActionSupport {
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

        GeraltRunner.run(project, root, GeraltCommand.BUILD, null);
    }
}
