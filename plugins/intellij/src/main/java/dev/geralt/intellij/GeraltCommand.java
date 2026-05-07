package dev.geralt.intellij;

import java.util.Locale;

public enum GeraltCommand {
    INIT("init", "Init"),
    BUILD("build", "Build"),
    RUN("run", "Run");

    private final String cliValue;
    private final String title;

    GeraltCommand(String cliValue, String title) {
        this.cliValue = cliValue;
        this.title = title;
    }

    public String cliValue() {
        return cliValue;
    }

    public String title() {
        return title;
    }

    public static GeraltCommand fromCliValue(String value) {
        for (GeraltCommand command : values()) {
            if (command.cliValue.equals(value.toLowerCase(Locale.ROOT))) {
                return command;
            }
        }
        throw new IllegalArgumentException("Unsupported Geralt command: " + value);
    }
}
