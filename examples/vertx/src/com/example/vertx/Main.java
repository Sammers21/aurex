package com.example.vertx;

import io.vertx.core.Vertx;

public class Main {
    public static void main(String[] args) {
        Vertx vertx = Vertx.vertx();
        System.out.println("Hello, Vert.x!");
        vertx.close().toCompletionStage().toCompletableFuture().join();
    }
}
