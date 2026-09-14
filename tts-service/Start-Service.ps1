param([string]$ModelId = 'qwen3-0.6b-customvoice')
$ErrorActionPreference = 'Stop'
$catalog = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'models.json') | ConvertFrom-Json
$model = $catalog.models | Where-Object id -EQ $ModelId | Select-Object -First 1
if (!$model) { throw "Unknown model: $ModelId" }
$serviceArgs = @('-u', (Join-Path $PSScriptRoot 'service.py'), '--model', $model.modelPath)
if ($model.revision) { $serviceArgs += @('--revision', $model.revision) }
# Stdio is the service protocol. The desktop normally owns this process directly.
& conda run --no-capture-output -n $model.condaEnv python @serviceArgs
exit $LASTEXITCODE
