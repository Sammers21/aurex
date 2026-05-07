plugins {
    java
    id("org.jetbrains.intellij.platform") version "2.16.0"
}

group = "dev.geralt"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdea("2023.3.8")
    }
}

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
}

intellijPlatform {
    buildSearchableOptions = false
    pluginConfiguration {
        id = "dev.geralt.ide"
        name = "Geralt"
        version = project.version.toString()
        description = "Adds Geralt init, build, run, and geralt.toml navigation actions."
        ideaVersion {
            sinceBuild = "233"
        }
        vendor {
            name = "Geralt"
        }
    }
}

tasks.register<JavaExec>("testProjectLocator") {
    dependsOn(tasks.testClasses)
    mainClass.set("dev.geralt.intellij.GeraltProjectLocatorTest")
    classpath = sourceSets["test"].runtimeClasspath
}

tasks.check {
    dependsOn("testProjectLocator")
}
