package com.example.text;

import java.util.Arrays;
import org.apache.commons.text.WordUtils;

public class Main {
    public static void main(String[] args) {
        String title = WordUtils.capitalizeFully("dependency driven text tools");
        int words = Arrays.asList(title.split(" ")).size();
        System.out.println(title + " (" + words + " words)");
    }
}
