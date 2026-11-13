; main.asm — главный модуль примера из двух модулей (main.asm + print.asm) для
; МОНИТОРА-F (полная конфигурация).
;
; Модуль выводит две строки подпрограммами библиотеки print.asm (PRINT — строка,
; NEWLINE — перевод строки) и выходит в МОНИТОР-F (тёплый старт F86CH). Сам модуль не
; знает адресов PRINT и NEWLINE (EXTRN) и своих адресов тоже: код лежит в CSEG, строки —
; в DSEG; адреса назначает компоновщик link-k580vm80a.
;
; Сборка (каждый модуль — в объектный файл .link, затем компоновка с адреса 3000H):
;   asm-k580vm80a -f examples/asm/hello-link/main.asm  -o main.link
;   asm-k580vm80a -f examples/asm/hello-link/print.asm -o print.link
;   link-k580vm80a main.link print.link -o hello-link.hex -m hello-link.map --base 3000
; или через make:
;   make asm F=examples/asm/hello-link/main.asm  O=examples/asm/hello-link/main.link
;   make asm F=examples/asm/hello-link/print.asm O=examples/asm/hello-link/print.link
;   make link O=examples/asm/hello-link/hello-link.hex B=3000 \
;        M="examples/asm/hello-link/main.link examples/asm/hello-link/print.link"
; (всё это делает make asm-examples). Запуск готового дампа:
;   target/release/ut88 --config full --load examples/asm/hello-link/hello-link.hex --run 3000

        NAME    MAIN
        EXTRN   PRINT,NEWLINE   ; из модуля PRINT
        PUBLIC  START

WARM    EQU     0F86CH          ; МОНИТОР-F: тёплый старт

        CSEG
START:  CALL    NEWLINE
        LXI     H,MSG1          ; адрес в DSEG — исправит компоновщик
        CALL    PRINT
        CALL    NEWLINE
        LXI     H,MSG2
        CALL    PRINT
        JMP     WARM            ; выход в МОНИТОР-F

        DSEG
MSG1:   DB      'HELLO FROM TWO MODULES!',0
MSG2:   DB      'ПРИВЕТ ИЗ ДВУХ МОДУЛЕЙ!',0

        END     START           ; адрес пуска — для компоновщика
