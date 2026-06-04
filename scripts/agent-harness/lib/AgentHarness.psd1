@{
    RootModule = 'AgentHarness.psm1'
    ModuleVersion = '0.1.0'
    GUID = 'cceaf91f-14e2-44f3-a431-c6d37fbf8071'
    Author = 'Hacienda'
    Description = 'Shared helpers for the Hacienda agent harness.'
    PowerShellVersion = '5.1'
    FunctionsToExport = @(
        'Get-AgentHarnessRepoRoot',
        'Read-AgentHarnessJsonFromStdin',
        'Get-AgentHarnessProperty',
        'Get-AgentHarnessCommandText',
        'Get-AgentHarnessPromptText',
        'Get-AgentHarnessFilePath',
        'Get-AgentHarnessCrateFromPath',
        'Get-AgentHarnessChangedRustFiles',
        'Get-AgentHarnessCratesFromPaths',
        'Get-AgentHarnessRustDiffFingerprint',
        'Test-AgentHarnessDangerousCommand',
        'Test-AgentHarnessSecretText',
        'Write-AgentHarnessBlockJson'
    )
    CmdletsToExport = @()
    VariablesToExport = '*'
    AliasesToExport = @()
}
