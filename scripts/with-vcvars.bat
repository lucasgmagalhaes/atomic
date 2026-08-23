@echo off
rem Sources the MSVC Developer Command Prompt environment, then runs
rem `cargo` with whatever arguments were passed through - see the
rem Makefile / CLAUDE.md for why quickjs-sys's C build needs this on this
rem machine. A separate .bat file (rather than inlining the `call` into
rem the Makefile recipe itself) sidesteps a real quoting problem: a
rem nested-quoted `cmd /c "..."` string passed through make's own shell
rem layer gets its escaped inner quotes mangled before cmd.exe ever sees
rem it, so cmd fails to find the vcvars64.bat path at all. One quoted
rem path here, no nesting, works.
if not defined NIMBLE_VCVARS set "NIMBLE_VCVARS=E:\VSBuildTools\VC\Auxiliary\Build\vcvars64.bat"
call "%NIMBLE_VCVARS%" >nul
cargo %*
