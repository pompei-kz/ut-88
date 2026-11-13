; draw.asm — рисование на экране дисплейного модуля (полная конфигурация).
;
; Программа пишет символы прямо в экранное ОЗУ: адрес знакоместа
; E800H + строка × 40H + столбец (экран 64 × 28, doc/29-displey-dlya-emulyatora.md).
; Рисует рамку из «*», две диагонали «\» и «/», инверсный (светлый) прямоугольник
; и надпись, затем ждёт нажатия любой клавиши (F803H) и выходит в МОНИТОР-F.
;
; Инверсия: бит 7 ячейки экранного ОЗУ инвертирует знакоместо. В ЮТ-88 инвертируется
; знакоместо СЛЕВА от ячейки с битом 7 (так МОНИТОР-F рисует курсор), поэтому
; прямоугольник, записанный в столбцы 24–40, светится в столбцах 23–39.
;
; Конфигурация: full (программа в ОЗУ с 3000H).
; Сборка:  make asm F=examples/asm/draw.asm
; Запуск:  make asm-run F=examples/asm/draw.asm

SCREEN  EQU     0E800H          ; начало экранного ОЗУ (строка 0, столбец 0)
GETCH   EQU     0F803H          ; МОНИТОР-F: ввод символа с клавиатуры в A
PUTCH   EQU     0F809H          ; МОНИТОР-F: вывод символа из C
WARM    EQU     0F86CH          ; МОНИТОР-F: тёплый старт (приглашение «=>»)
CLS     EQU     1FH             ; управляющий код «СТР» — очистка экрана
INVSP   EQU     0A0H            ; пробел с битом 7 — инверсное знакоместо

; PUT строка, столбец, символ — один символ в заданное место
PUT     MACRO   ROW, COL, CH
        MVI     B,ROW
        MVI     C,COL
        MVI     E,CH
        CALL    PLOT
        ENDM

        ORG     3000H
START:  MVI     C,CLS           ; очистить экран средствами МОНИТОРА-F
        CALL    PUTCH

; --- рамка: строки 0 и 25, столбцы 0 и 63 (строки 26–27 — для приглашения МОНИТОРА)
        MVI     B,0
        MVI     C,0
        MVI     D,64
        MVI     E,'*'
        CALL    HLINE
        MVI     B,25
        MVI     C,0
        MVI     D,64
        CALL    HLINE
        MVI     B,1
        MVI     C,0
        MVI     D,24
        CALL    VLINE
        MVI     B,1
        MVI     C,63
        MVI     D,24
        CALL    VLINE

; --- диагонали: строка 1+i, столбцы 8+2i («\») и 55-2i («/»), i = 0…23
        MVI     B,1             ; строка
        MVI     C,8             ; столбец «\»
        MVI     D,55            ; столбец «/»
DIAG:   MVI     E,'\'
        CALL    PLOT
        PUSH    B
        MOV     C,D
        MVI     E,'/'
        CALL    PLOT
        POP     B
        INR     C               ; «\» — вправо на два столбца
        INR     C
        DCR     D               ; «/» — влево на два столбца
        DCR     D
        INR     B
        MOV     A,B
        CPI     25              ; до строки 24 включительно
        JNZ     DIAG

; --- инверсный прямоугольник: строки 10–14, ячейки 24–40
        MVI     B,10
BOX:    MVI     C,24
        MVI     D,17
        MVI     E,INVSP
        CALL    HLINE
        INR     B
        MOV     A,B
        CPI     15
        JNZ     BOX

; --- надпись в строке 20 по центру
        MVI     B,20
        MVI     C,25
        LXI     H,TITLE
TEXT:   MOV     A,M
        ORA     A
        JZ      MARKS
        MOV     E,A
        CALL    PLOT
        INR     C
        INX     H
        JMP     TEXT

; --- уголки рамки другим символом — пример макроса PUT
MARKS:  PUT     0, 0, '+'
        PUT     0, 63, '+'
        PUT     25, 0, '+'
        PUT     25, 63, '+'

; --- ждать клавишу и выйти в МОНИТОР-F
        CALL    GETCH
        JMP     WARM

; HLINE: горизонтальная линия. B — строка, C — столбец, D — длина, E — символ.
HLINE:  CALL    PLOT
        INR     C
        DCR     D
        JNZ     HLINE
        RET

; VLINE: вертикальная линия. B — строка, C — столбец, D — длина, E — символ.
VLINE:  CALL    PLOT
        INR     B
        DCR     D
        JNZ     VLINE
        RET

; PLOT: символ E в строку B (0–27), столбец C (0–63).
; Адрес = E800H + B × 40H + C: младший байт = (B AND 3) × 40H + C,
; старший = E8H + B / 4. Сохраняет все регистры, кроме A и флагов.
PLOT:   PUSH    H
        MOV     A,B
        ANI     3
        RRC                     ; (B AND 3) сдвигается в биты 7–6
        RRC
        ORA     C               ; + столбец
        MOV     L,A
        MOV     A,B
        RRC                     ; B / 4
        RRC
        ANI     3FH
        ADI     HIGH SCREEN
        MOV     H,A
        MOV     M,E
        POP     H
        RET

TITLE:  DB      'ЮТ-88 РИСУЕТ!',0
        END     START
