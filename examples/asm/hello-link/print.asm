; print.asm — библиотека вывода для примера из двух модулей (см. main.asm).
;
; PRINT   — вывести строку: HL — адрес, конец — байт 00H. Портит A, C, HL.
; NEWLINE — перевести строку (коды 0DH, 0AH). Портит A, C.
; Символы выводит подпрограмма МОНИТОРА-F F809H (вывод символа из C).
;
; Сборка: asm-k580vm80a -f examples/asm/hello-link/print.asm -o print.link
; (сам по себе модуль не запускается — его подключает компоновщик).

        NAME    PRINT
        PUBLIC  PRINT,NEWLINE

PUTCH   EQU     0F809H          ; МОНИТОР-F: вывод символа из C

        CSEG
PRINT:  MOV     A,M             ; очередной символ
        ORA     A               ; 0 — конец строки
        RZ
        MOV     C,A
        PUSH    H
        CALL    PUTCH
        POP     H
        INX     H
        JMP     PRINT

NEWLINE:
        PUSH    H
        LXI     H,CRLF          ; строка из своего DSEG
        CALL    PRINT
        POP     H
        RET

        DSEG
CRLF:   DB      0DH,0AH,0

        END
