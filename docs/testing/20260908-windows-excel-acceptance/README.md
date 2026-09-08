# Windows Excel acceptance for 0.2.1

Status: prepared; desktop Excel validation pending.

neatxlsx 0.2.1 — Windows Excel 验证（不需要安装 Python）

请使用桌面版 Microsoft Excel，先记录版本号（文件 → 账户 → 关于 Excel）。

1. 打开 display-autofit-acceptance.xlsx。
   记录是否直接打开、是否出现“发现不可读取内容”或修复提示。
   如果有修复提示，取消修复并保留提示内容；再打开 nonzip64 对照文件，记录差异。

2. 在 Display matrix 工作表以 100% 缩放检查两行数据。
   重点看中/日/韩/全角文本、decimal/grouping/percent/currency、日期和时间。
   记录截字、#####、异常格式；emoji 字形也请记录，字体差异单独注明。
   不要先双击列边界自动调整，否则看不到库生成的原始列宽。

3. Bounds 工作表中的超长 W 字符串被最大列宽限制，显示不全是预期。
   Sampling 第三行数据（长英文串）在采样范围外，显示不全也是预期。

4. 在 BestFit edit 工作表，把 A2 从 1234.56 改为 1234567890123.45 后按回车。
   记录列 A 是否自动变宽，还是显示 ##### / 截断。
   该数值不超过 Excel 的 15 位有效数字限制。
   如果没有自动变宽，照实记录；此步骤是验证行为，不要求手工调整成通过。

5. 另存为 edited-in-excel.xlsx，关闭后重新打开，确认仍能打开且改动保留。

回复模板：
Excel 版本：
默认 ZIP64 文件打开：正常 / 有修复提示（内容）
nonzip64 对照（仅必要时）：
原始文本及数值显示：正常 / 异常（工作表、单元格）
A2 改值后列宽：自动变宽 / 不变（显示情况）
另存后重开：正常 / 异常

两份 xlsx 都是无宏的示例数据。generation.json 记录生成包及文件身份。

The candidate was generated with the local 0.2.1 release wheel. Structural
readback with openpyxl confirms both workbooks open, contain four expected
sheets, and have A2=1234.56 with bestFit metadata on column A. This does not
establish desktop display or editing behavior. The short numeric header and
15-significant-digit edit value avoid pre-widening the column or testing
Excel precision loss instead of width behavior.
