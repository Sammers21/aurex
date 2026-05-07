package dev.geralt.intellij.actions;

import com.intellij.notification.NotificationType;
import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import dev.geralt.intellij.GeraltCommand;
import dev.geralt.intellij.GeraltProjectLocator;
import dev.geralt.intellij.GeraltRunner;
import java.nio.file.Files;
import java.nio.file.Path;
import org.jetbrains.annotations.NotNull;

public final class InitGeraltProjectAction extends GeraltActionSupport {
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

        if (Files.exists(root.resolve(GeraltProjectLocator.GERALT_TOML))) {
            GeraltRunner.notify(
                    project,
                    "Geralt project already exists",
                    "geralt.toml already exists in " + root,
                    NotificationType.WARNING
            );
            return;
        }

        GeraltRunner.run(project, root, GeraltCommand.INIT, () -> openManifest(project, root));
    }
}
