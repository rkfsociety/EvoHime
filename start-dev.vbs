Set shell = CreateObject("WScript.Shell")
Set fs = CreateObject("Scripting.FileSystemObject")
powershell = shell.ExpandEnvironmentStrings("%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe")
script = fs.GetAbsolutePathName("start-dev.ps1")
shell.Run """" & powershell & """ -NoProfile -ExecutionPolicy Bypass -File """ & script & """", 0, False
