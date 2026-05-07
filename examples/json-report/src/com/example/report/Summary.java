package com.example.report;

public class Summary {
    public final String domain;
    public final int checks;
    public final boolean passing;

    public Summary(String domain, int checks, boolean passing) {
        this.domain = domain;
        this.checks = checks;
        this.passing = passing;
    }
}
