package com.example.orders;

import picocli.CommandLine;
import picocli.CommandLine.Command;
import picocli.CommandLine.Option;

@Command(name = "orders")
public class Main implements Runnable {
    @Option(names = "--region", defaultValue = "north")
    String region;

    @Option(names = "--priority", defaultValue = "2")
    int priority;

    @Override
    public void run() {
        System.out.println(region + " priority " + priority + ": 5 orders");
    }

    public static void main(String[] args) {
        int exitCode = new CommandLine(new Main()).execute("--region", "north", "--priority", "2");
        if (exitCode != 0) {
            System.exit(exitCode);
        }
    }
}
