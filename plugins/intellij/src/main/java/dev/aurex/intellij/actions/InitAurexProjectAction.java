package dev.aurex.intellij.actions;

import com.intellij.notification.NotificationType;
import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import dev.aurex.intellij.AurexCommand;
import dev.aurex.intellij.AurexProjectLocator;
import dev.aurex.intellij.AurexRunner;
import java.nio.file.Files;
import java.nio.file.Path;
import org.jetbrains.annotations.NotNull;

public final class InitAurexProjectAction extends AurexActionSupport {
    @Override
    public void actionPerformed(@NotNull AnActionEvent event) {
        Project project = event.getProject();
        if (project == null) {
            return;
        }

        Path root = initRoot(event).orElse(null);
        if (root == null) {
            warnNoProject(project);
            return;
        }

        if (Files.exists(root.resolve(AurexProjectLocator.AX_TOML))) {
            AurexRunner.notify(
                    project,
                    "Aurex project already exists",
                    "ax.toml already exists in " + root,
                    NotificationType.WARNING
            );
            return;
        }

        AurexRunner.run(project, root, AurexCommand.INIT, () -> openManifest(project, root));
    }
}
