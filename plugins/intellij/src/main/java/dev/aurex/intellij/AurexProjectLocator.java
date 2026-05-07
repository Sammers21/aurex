package dev.aurex.intellij;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Optional;
import java.util.Set;
import java.util.stream.Stream;

public final class AurexProjectLocator {
    public static final String AX_TOML = "ax.toml";

    private static final Set<String> IGNORED_DIRECTORIES = Set.of(
            ".git",
            ".idea",
            "build",
            "node_modules",
            "out",
            "target"
    );

    private AurexProjectLocator() {
    }

    public static boolean isAurexProject(Path directory) {
        return directory != null && Files.isRegularFile(directory.resolve(AX_TOML));
    }

    public static Optional<Path> nearestRoot(Path start, Collection<Path> workspaceBoundaries) {
        if (start == null) {
            return Optional.empty();
        }

        Set<Path> boundaries = new HashSet<>();
        for (Path boundary : workspaceBoundaries) {
            if (boundary != null) {
                boundaries.add(boundary.toAbsolutePath().normalize());
            }
        }

        for (Path current = directoryFor(start).toAbsolutePath().normalize();
             current != null;
             current = current.getParent()) {
            if (isAurexProject(current)) {
                return Optional.of(current);
            }
            if (boundaries.contains(current)) {
                return Optional.empty();
            }
        }
        return Optional.empty();
    }

    public static List<Path> discoverRoots(Collection<Path> workspaceRoots) {
        List<Path> projects = new ArrayList<>();
        for (Path workspaceRoot : workspaceRoots) {
            walk(workspaceRoot.toAbsolutePath().normalize(), projects);
        }
        projects.sort(Comparator.naturalOrder());
        return projects;
    }

    public static Path directoryFor(Path candidate) {
        if (candidate == null) {
            throw new IllegalArgumentException("candidate path is required");
        }
        return Files.isDirectory(candidate) ? candidate : candidate.getParent();
    }

    private static void walk(Path directory, List<Path> projects) {
        if (directory == null || !Files.isDirectory(directory)) {
            return;
        }

        if (isAurexProject(directory)) {
            projects.add(directory);
        }

        try (Stream<Path> children = Files.list(directory)) {
            children
                    .filter(Files::isDirectory)
                    .filter(child -> !IGNORED_DIRECTORIES.contains(child.getFileName().toString()))
                    .forEach(child -> walk(child, projects));
        } catch (IOException ignored) {
            // Broken or unreadable folders should not make the IDE action fail.
        }
    }
}
