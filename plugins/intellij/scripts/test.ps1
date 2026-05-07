$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$classes = Join-Path $root "build\self-test\classes"

if (Test-Path -LiteralPath $classes) {
    Remove-Item -LiteralPath $classes -Recurse -Force
}
New-Item -ItemType Directory -Path $classes | Out-Null

javac -d $classes `
    (Join-Path $root "src\main\java\dev\aurex\intellij\AurexCommand.java") `
    (Join-Path $root "src\main\java\dev\aurex\intellij\AurexProjectLocator.java") `
    (Join-Path $root "src\test\java\dev\aurex\intellij\AurexProjectLocatorTest.java")

java -cp $classes dev.aurex.intellij.AurexProjectLocatorTest
