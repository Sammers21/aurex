package dev.aurex.intellij;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

public final class AurexProjectLocatorTest {
    public static void main(String[] args) throws Exception {
        findsNearestProjectFromNestedFiles();
        stopsAtWorkspaceBoundary();
        discoversNestedProjectsAndSkipsGeneratedFolders();
        validatesCommands();
        validatesPluginXmlActions();
        System.out.println("Aurex IntelliJ project locator tests passed");
    }

    private static void findsNearestProjectFromNestedFiles() throws IOException {
        Path root = tempDir();
        Path project = root.resolve("service");
        Path source = project.resolve("src/com/example/Main.java");
        Files.createDirectories(source.getParent());
        Files.writeString(project.resolve(AurexProjectLocator.AUREX_TOML), "[package]\nname = \"service\"\n");
        Files.writeString(source, "class Main {}\n");

        assertEquals(
                project,
                AurexProjectLocator.nearestRoot(source, List.of(root)).orElseThrow(),
                "nearest project root"
        );
    }

    private static void stopsAtWorkspaceBoundary() throws IOException {
        Path root = tempDir();
        Path workspace = root.resolve("workspace");
        Path source = workspace.resolve("src/Main.java");
        Files.createDirectories(source.getParent());
        Files.writeString(root.resolve(AurexProjectLocator.AUREX_TOML), "[package]\nname = \"outside\"\n");
        Files.writeString(source, "class Main {}\n");

        assertTrue(
                AurexProjectLocator.nearestRoot(source, List.of(workspace)).isEmpty(),
                "workspace boundary should stop upward search"
        );
    }

    private static void discoversNestedProjectsAndSkipsGeneratedFolders() throws IOException {
        Path root = tempDir();
        Path api = root.resolve("api");
        Path cli = root.resolve("tools/cli");
        Path generated = root.resolve("target/ignored");
        for (Path project : List.of(api, cli, generated)) {
            Files.createDirectories(project);
            Files.writeString(project.resolve(AurexProjectLocator.AUREX_TOML), "[package]\nname = \"demo\"\n");
        }

        assertEquals(
                List.of(api, cli),
                AurexProjectLocator.discoverRoots(List.of(root)),
                "discovered project roots"
        );
    }

    private static void validatesCommands() {
        assertEquals(AurexCommand.BUILD, AurexCommand.fromCliValue("build"), "build command");
        assertThrows(() -> AurexCommand.fromCliValue("delete"), "invalid command");
    }

    private static void validatesPluginXmlActions() throws IOException {
        String pluginXml = Files.readString(Path.of("src/main/resources/META-INF/plugin.xml"));

        assertContains(pluginXml, "Aurex.Init", "init action");
        assertContains(pluginXml, "Aurex.Build", "build action");
        assertContains(pluginXml, "Aurex.Run", "run action");
        assertContains(pluginXml, "Aurex.OpenToml", "open action");
        assertContains(pluginXml, "ToolsMenu", "tools menu registration");
        assertContains(pluginXml, "ProjectViewPopupMenu", "project view registration");
    }

    private static Path tempDir() throws IOException {
        return Files.createTempDirectory("aurex-intellij-");
    }

    private static void assertEquals(Object expected, Object actual, String label) {
        if (!expected.equals(actual)) {
            throw new AssertionError(label + ": expected <" + expected + "> but was <" + actual + ">");
        }
    }

    private static void assertTrue(boolean condition, String label) {
        if (!condition) {
            throw new AssertionError(label);
        }
    }

    private static void assertThrows(Runnable runnable, String label) {
        try {
            runnable.run();
        } catch (IllegalArgumentException expected) {
            return;
        }
        throw new AssertionError(label + ": expected IllegalArgumentException");
    }

    private static void assertContains(String haystack, String needle, String label) {
        if (!haystack.contains(needle)) {
            throw new AssertionError(label + ": expected to contain <" + needle + ">");
        }
    }
}
