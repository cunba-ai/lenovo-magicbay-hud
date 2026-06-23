@echo off
rem 启动 magicbay-hud。exe 已内嵌 requireAdministrator 清单，会自动弹 UAC 请求管理员权限。
rem （CPU 温度/功率需管理员 + PawnIO 驱动；按 Esc 关闭窗口。）
start "" "%~dp0target\release\magicbay-hud.exe"
