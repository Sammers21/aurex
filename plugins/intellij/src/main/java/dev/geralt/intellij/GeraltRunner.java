package dev.geralt.intellij;

import com.intellij.execution.ExecutionException;
import com.intellij.execution.configurations.GeneralCommandLine;
import com.intellij.execution.executors.DefaultRunExecutor;
import com.intellij.execution.filters.TextConsoleBuilderFactory;
import com.intellij.execution.process.OSProcessHandler;
import com.intellij.execution.process.ProcessEvent;
import com.intellij.execution.process.ProcessHandlerFactory;
import com.intellij.execution.process.ProcessListener;
import com.intellij.execution.process.ProcessTerminatedListener;
import com.intellij.execution.ui.ConsoleView;
import com.intellij.execution.ui.RunContentDescriptor;
import com.intellij.execution.ui.RunContentManager;
import com.intellij.notification.NotificationGroupManager;
import com.intellij.notification.NotificationType;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.Key;
import java.nio.file.Path;
import org.jetbrains.annotations.NotNull;

public final class GeraltRunner {
    private GeraltRunner() {
    }

    public static void run(Project project, Path cwd, GeraltCommand command, Runnable onSuccess) {
        try {
            GeneralCommandLine commandLine = new GeneralCommandLine(resolveExecutable(), command.cliValue())
                    .withWorkDirectory(cwd.toFile())
                    .withParentEnvironmentType(GeneralCommandLine.ParentEnvironmentType.CONSOLE);

            OSProcessHandler handler = ProcessHandlerFactory.getInstance()
                    .createColoredProcessHandler(commandLine);
            ProcessTerminatedListener.attach(handler);

            ConsoleView console = TextConsoleBuilderFactory.getInstance()
                    .createBuilder(project)
                    .getConsole();
            console.attachToProcess(handler);

            RunContentDescriptor descriptor = new RunContentDescriptor(
                    console,
                    handler,
                    console.getComponent(),
                    "Geralt: " + command.title()
            );
            RunContentManager.getInstance(project)
                    .showRunContent(DefaultRunExecutor.getRunExecutorInstance(), descriptor);

            if (onSuccess != null) {
                handler.addProcessListener(new ProcessListener() {
                    @Override
                    public void processTerminated(@NotNull ProcessEvent event) {
                        if (event.getExitCode() == 0 && !project.isDisposed()) {
                            ApplicationManager.getApplication().invokeLater(onSuccess);
                        }
                    }

                    @Override
                    public void startNotified(@NotNull ProcessEvent event) {
                    }

                    @Override
                    public void processWillTerminate(@NotNull ProcessEvent event, boolean willBeDestroyed) {
                    }

                    @Override
                    public void onTextAvailable(@NotNull ProcessEvent event, @NotNull Key outputType) {
                    }
                });
            }

            handler.startNotify();
        } catch (ExecutionException ex) {
            notify(project, "Failed to run Geralt", ex.getMessage(), NotificationType.ERROR);
        }
    }

    public static void notify(Project project, String title, String content, NotificationType type) {
        NotificationGroupManager.getInstance()
                .getNotificationGroup("Geralt")
                .createNotification(title, content, type)
                .notify(project);
    }

    private static String resolveExecutable() {
        String configured = System.getenv("GERALT_EXECUTABLE");
        return configured == null || configured.isBlank() ? "geralt" : configured;
    }
}
