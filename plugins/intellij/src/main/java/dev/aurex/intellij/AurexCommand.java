package dev.aurex.intellij;

import java.util.Locale;

public enum AurexCommand {
    INIT("init", "Init"),
    BUILD("build", "Build"),
    RUN("run", "Run"),
    TEST("test", "Test"),
    CLEAN("clean", "Clean"),
    FMT("fmt", "Format");

    private final String cliValue;
    private final String title;

    AurexCommand(String cliValue, String title) {
        this.cliValue = cliValue;
        this.title = title;
    }

    public String cliValue() {
        return cliValue;
    }

    public String title() {
        return title;
    }

    public static AurexCommand fromCliValue(String value) {
        for (AurexCommand command : values()) {
            if (command.cliValue.equals(value.toLowerCase(Locale.ROOT))) {
                return command;
            }
        }
        throw new IllegalArgumentException("Unsupported Aurex command: " + value);
    }
}
