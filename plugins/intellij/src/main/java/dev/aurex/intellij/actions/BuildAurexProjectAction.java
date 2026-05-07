package dev.aurex.intellij.actions;

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import dev.aurex.intellij.AurexCommand;
import dev.aurex.intellij.AurexRunner;
import java.nio.file.Path;
import org.jetbrains.annotations.NotNull;

public final class BuildAurexProjectAction extends AurexActionSupport {
    @Override
    public void actionPerformed(@NotNull AnActionEvent event) {
        Project project = event.getProject();
        if (project == null) {
            return;
        }

        Path root = existingAurexRoot(event).orElse(null);
        if (root == null) {
            warnNoProject(project);
            return;
        }

        AurexRunner.run(project, root, AurexCommand.BUILD, null);
    }
}
