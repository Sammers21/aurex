plugins {
    java
    id("org.jetbrains.intellij.platform") version "2.16.0"
}

group = "dev.aurex"
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
        id = "dev.aurex.ide"
        name = "Aurex"
        version = project.version.toString()
        description = "Adds Aurex init, build, run, and ax.toml navigation actions."
        ideaVersion {
            sinceBuild = "233"
        }
        vendor {
            name = "Aurex"
        }
    }
}

tasks.register<JavaExec>("testProjectLocator") {
    dependsOn(tasks.testClasses)
    mainClass.set("dev.aurex.intellij.AurexProjectLocatorTest")
    classpath = sourceSets["test"].runtimeClasspath
}

tasks.check {
    dependsOn("testProjectLocator")
}
