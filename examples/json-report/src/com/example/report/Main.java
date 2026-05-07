package com.example.report;

import com.google.gson.Gson;

public class Main {
    public static void main(String[] args) {
        Summary summary = new Summary("build", 3, true);
        System.out.println(new Gson().toJson(summary));
    }
}
