$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$classes = Join-Path $root "build\self-test\classes"

if (Test-Path -LiteralPath $classes) {
    Remove-Item -LiteralPath $classes -Recurse -Force
}
New-Item -ItemType Directory -Path $classes | Out-Null

javac -d $classes `
    (Join-Path $root "src\main\java\dev\geralt\intellij\GeraltCommand.java") `
    (Join-Path $root "src\main\java\dev\geralt\intellij\GeraltProjectLocator.java") `
    (Join-Path $root "src\test\java\dev\geralt\intellij\GeraltProjectLocatorTest.java")

java -cp $classes dev.geralt.intellij.GeraltProjectLocatorTest
