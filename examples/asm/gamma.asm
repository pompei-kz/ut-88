; gamma.asm — программа «Гамма» (журнал 1989-02, doc/07-programmy.md). Исходный текст
; восстановлен по дампу; собирается байт-в-байт в programs/gamma.bin (C000H–C082H).
;
; Программа играет гамму из 8 нот через выход на магнитофон (порт A1H, бит 0). После
; пуска с адреса C03EH одним вызовом RST 6 вводятся подряд четыре цифры, например 1002:
; первые две (10H) — длительность ноты, вторые две (02H) — пауза между нотами. Затем
; гамма играет по кругу без остановки; выход — «УСТ». В эмуляторе звук слышен через
; динамик ПК.
;
; Конфигурация: модуль 1 (minimal) + магнитофон.
; Сборка:  make asm F=examples/asm/gamma.asm          (дамп examples/asm/gamma.hex)
; Запуск:  make asm-run F=examples/asm/gamma.asm      (пуск с C03EH — из END), затем
;          набрать на клавиатуре модуля 1 четыре цифры, например 1002.
; На ЮТ-88: ввести коды директивой «1» с C000H, пуск — директивой «7» с адреса C03E.

TAPE    EQU     0A1H            ; порт выхода на магнитофон
TONECNT EQU     0CC9H           ; счётчик длительности одного «такта» ноты
PAUSCNT EQU     0CCAH           ; счётчик задержки одного «такта» паузы

        ORG     0C000H
        NOP                     ; три пустых байта в начале — как в журнале
        NOP
        NOP

; TONE — сыграть ноту. C — полупериод (меньше — выше), D — длительность.
TONE:   PUSH    D
        MVI     E,1             ; E — уровень выхода (бит 0)
TONE1:  LXI     H,TONECNT       ; HL уменьшается на C за каждый полупериод
TONE2:  MOV     A,E
        OUT     TAPE            ; выдать уровень
        CALL    HALF            ; полупериод: C циклов, HL = HL - C
        CMA                     ; инвертировать уровень
        MOV     E,A
        NOP
        MOV     A,H             ; пока старший байт HL не 0 — продолжать
        ORA     H
        JNZ     TONE2
        DCR     D               ; следующий «такт» длительности
        JNZ     TONE1
        POP     D
        RET

; HALF — задержка на полупериод: C раз уменьшить HL.
HALF:   MOV     B,C
HALF1:  DCX     H
        DCR     B
        JNZ     HALF1
        RET

; DELAY — одиночная задержка (в программе не вызывается).
DELAY:  LXI     H,PAUSCNT
DELAY1: DCX     H
        MOV     A,L
        ORA     H
        JNZ     DELAY1
        RET

; PAUSE — пауза между нотами, E «тактов».
PAUSE:  PUSH    D
PAUSE1: LXI     H,PAUSCNT
PAUSE2: DCX     H
        MOV     A,L
        ORA     H
        JNZ     PAUSE2
        DCR     E
        JNZ     PAUSE1
        POP     D
        RET

; Пуск: ввести длительность (D) и паузу (E) четырьмя цифрами, затем играть по кругу.
START:  RST     6               ; DE = введённые цифры
PLAY:   MVI     C,8BH           ; нота 1
        CALL    TONE
        CALL    PAUSE
        MVI     C,7AH           ; нота 2
        CALL    TONE
        CALL    PAUSE
        MVI     C,6CH           ; нота 3
        CALL    TONE
        CALL    PAUSE
        MVI     C,65H           ; нота 4
        CALL    TONE
        CALL    PAUSE
        MVI     C,5AH           ; нота 5
        CALL    TONE
        CALL    PAUSE
        MVI     C,4FH           ; нота 6
        INX     B               ; лишний байт 03H по адресу C069H, как напечатан в
                                ; журнале: C = 50H, нота звучит чуть ниже (~1 %).
                                ; Оставлен, чтобы образ совпал с журналом байт-в-байт
        CALL    TONE
        CALL    PAUSE
        MVI     C,47H           ; нота 7
        CALL    TONE
        CALL    PAUSE
        MVI     C,44H           ; нота 8
        CALL    TONE
        CALL    PAUSE
        JMP     PLAY            ; по кругу
        END     START
