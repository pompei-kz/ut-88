; macro.asm — пример препроцессора ассемблера: MACRO (с параметрами и LOCAL), REPT,
; IF/ELSE/ENDIF и IFDEF.
;
; Программа для модуля 1 считает сумму квадратов 1² + 2² + … + N² по таблице,
; построенной директивой REPT, и выводит на индикаторы (слева направо):
;   левая пара — N, средняя — 00 (или 01 с ключом -D TWICE), правая — сумма.
; При N = 5 на индикаторах «05 00 37» (1 + 4 + 9 + 16 + 25 = 55 = 37H).
; После вывода — пауза (макрос DELAY) и возврат в МОНИТОР (RST 0).
;
; Конфигурация: модуль 1 (minimal).
; Сборка:  make asm F=examples/asm/macro.asm             (дамп examples/asm/macro.hex)
;          asm-k580vm80a -f examples/asm/macro.asm -o macro.hex -l macro.lst
;                        (листинг: «+» — строки раскрытия макросов и REPT,
;                         «-» — несобранные строки ложной ветки IF)
;          asm-k580vm80a -f examples/asm/macro.asm -o macro.hex -D TWICE
;                        (сумма удваивается, в средней паре — 01)
; Запуск:  make asm-run F=examples/asm/macro.asm

DISP    EQU     9000H           ; индикаторы: +0 — правый, +1 — средний, +2 — левый
N       EQU     5               ; сколько квадратов складывать (1…6: сумма — байт)

; SHOW POS,VAL — вывести константу VAL в пару индикаторов POS (0 — правая).
; Для VAL = 0 вместо MVI A,0 собирается более короткая XRA A.
SHOW    MACRO   POS,VAL
        IF      VAL = 0
        XRA     A
        ELSE
        MVI     A,VAL
        ENDIF
        STA     DISP+POS
        ENDM

; DELAY CNT — задержка: CNT раз по 65536 проходов внутреннего цикла (≈0,9 с при
; частоте процессора 1,78 МГц).
; LOCAL даёт меткам уникальные имена при каждом раскрытии.
DELAY   MACRO   CNT
        LOCAL   OUTER,INNER
        MVI     C,CNT
OUTER:  LXI     D,0
INNER:  DCX     D
        MOV     A,D
        ORA     E
        JNZ     INNER
        DCR     C
        JNZ     OUTER
        ENDM

        ORG     0C000H
START:  SHOW    2,N             ; левая пара — N
        IFDEF   TWICE
        SHOW    1,1             ; средняя пара — признак удвоения
        ELSE
        SHOW    1,0
        ENDIF
        LXI     H,SQUARES       ; сумма квадратов по таблице
        MVI     B,N
        XRA     A
SUM:    ADD     M
        INX     H
        DCR     B
        JNZ     SUM
        IFDEF   TWICE
        ADD     A               ; удвоить сумму
        ENDIF
        STA     DISP            ; правая пара — сумма
        DELAY   10              ; около 9 с, чтобы рассмотреть результат
        RST     0               ; возврат в МОНИТОР

; Таблица квадратов 1², 2², …, N² — строится при сборке.
SQUARES:
I       SET     0
        REPT    N
I       SET     I+1
        DB      I*I
        ENDM

        END     START
