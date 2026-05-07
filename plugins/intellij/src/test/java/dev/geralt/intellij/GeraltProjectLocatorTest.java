package dev.geralt.intellij;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

public final class GeraltProjectLocatorTest {
    public static void main(String[] args) throws Exception {
        findsNearestProjectFromNestedFiles();
        stopsAtWorkspaceBoundary();
        discoversNestedProjectsAndSkipsGeneratedFolders();
        validatesCommands();
        validatesPluginXmlActions();
        System.out.println("Geralt IntelliJ project locator tests passed");
    }

    private static void findsNearestProjectFromNestedFiles() throws IOException {
        Path root = tempDir();
        Path project = root.resolve("service");
        Path source = project.resolve("src/com/example/Main.java");
        Files.createDirectories(source.getParent());
        Files.writeString(project.resolve(GeraltProjectLocator.GERALT_TOML), "[package]\nname = \"service\"\n");
        Files.writeString(source, "class Main {}\n");

        assertEquals(
                project,
                GeraltProjectLocator.nearestRoot(source, List.of(root)).orElseThrow(),
                "nearest project root"
        );
    }

    private static void stopsAtWorkspaceBoundary() throws IOException {
        Path root = tempDir();
        Path workspace = root.resolve("workspace");
        Path source = workspace.resolve("src/Main.java");
        Files.createDirectories(source.getParent());
        Files.writeString(root.resolve(GeraltProjectLocator.GERALT_TOML), "[package]\nname = \"outside\"\n");
        Files.writeString(source, "class Main {}\n");

        assertTrue(
                GeraltProjectLocator.nearestRoot(source, List.of(workspace)).isEmpty(),
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
            Files.writeString(project.resolve(GeraltProjectLocator.GERALT_TOML), "[package]\nname = \"demo\"\n");
        }

        assertEquals(
                List.of(api, cli),
                GeraltProjectLocator.discoverRoots(List.of(root)),
                "discovered project roots"
        );
    }

    private static void validatesCommands() {
        assertEquals(GeraltCommand.BUILD, GeraltCommand.fromCliValue("build"), "build command");
        assertThrows(() -> GeraltCommand.fromCliValue("delete"), "invalid command");
    }

    private static void validatesPluginXmlActions() throws IOException {
        String pluginXml = Files.readString(Path.of("src/main/resources/META-INF/plugin.xml"));

        assertContains(pluginXml, "Geralt.Init", "init action");
        assertContains(pluginXml, "Geralt.Build", "build action");
        assertContains(pluginXml, "Geralt.Run", "run action");
        assertContains(pluginXml, "Geralt.OpenToml", "open action");
        assertContains(pluginXml, "ToolsMenu", "tools menu registration");
        assertContains(pluginXml, "ProjectViewPopupMenu", "project view registration");
    }

    private static Path tempDir() throws IOException {
        return Files.createTempDirectory("geralt-intellij-");
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
