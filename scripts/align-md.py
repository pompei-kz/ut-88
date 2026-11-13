#!/usr/bin/env python3
"""Выравнивание таблиц Markdown по правилу CLAUDE.md.

Использование: scripts/align-md.py [--code] ФАЙЛ.md ...
  --code  выравнивать и таблицы внутри блоков ``` (например, примеры в CLAUDE.md)

Ширина считается по символам (кириллица — 1 символ, широкие символы — 2); код в
обратных кавычках внутри ячеек и экранированные \\| разделителями не считаются.
"""
import re
import sys
import unicodedata


def width(s):
    w = 0
    for ch in s:
        if unicodedata.combining(ch):
            continue
        w += 2 if unicodedata.east_asian_width(ch) in ('W', 'F') else 1
    return w


def split_row(line):
    body = line.strip()
    body = body[1:-1] if body.endswith('|') and not body.endswith('\\|') else body[1:]
    cells, cur, i = [], '', 0
    while i < len(body):
        c = body[i]
        if c == '`':
            n = 1
            while i + n < len(body) and body[i + n] == '`':
                n += 1
            fence = '`' * n
            j = body.find(fence, i + n)
            while j != -1 and j + n < len(body) and body[j + n] == '`':
                j = body.find(fence, j + n + 1)
            if j == -1:
                cur += fence
                i += n
                continue
            cur += body[i:j + n]
            i = j + n
            continue
        if c == '\\' and i + 1 < len(body):
            cur += body[i:i + 2]
            i += 2
            continue
        if c == '|':
            cells.append(cur.strip())
            cur = ''
        else:
            cur += c
        i += 1
    cells.append(cur.strip())
    return cells


SEP = re.compile(r'^\s*:?-+:?\s*$')


def align(rows):
    cells = [split_row(r) for r in rows]
    n = max(len(c) for c in cells)
    for c in cells:
        c += [''] * (n - len(c))
    aligns = []
    for s in cells[1]:
        s = s.strip()
        if s.startswith(':') and s.endswith(':'):
            aligns.append('c')
        elif s.endswith(':'):
            aligns.append('r')
        elif s.startswith(':'):
            aligns.append('l')
        else:
            aligns.append('')
    w = [max(3, *(width(c[i]) for k, c in enumerate(cells) if k != 1)) for i in range(n)]
    out = []
    for k, c in enumerate(cells):
        if k == 1:
            parts = []
            for i in range(n):
                a, d = aligns[i], '-' * (w[i] + 2)
                if a == 'c':
                    d = ':' + '-' * w[i] + ':'
                elif a == 'r':
                    d = '-' * (w[i] + 1) + ':'
                elif a == 'l':
                    d = ':' + '-' * (w[i] + 1)
                parts.append(d)
            out.append('|' + '|'.join(parts) + '|')
            continue
        parts = []
        for i in range(n):
            pad = w[i] - width(c[i])
            if aligns[i] == 'r':
                t = ' ' * pad + c[i]
            elif aligns[i] == 'c':
                t = ' ' * (pad // 2) + c[i] + ' ' * (pad - pad // 2)
            else:
                t = c[i] + ' ' * pad
            parts.append(t)
        out.append('| ' + ' | '.join(parts) + ' |')
    return out


def process(text, in_code_too=False):
    lines = text.split('\n')
    res, i, fence = [], 0, False
    while i < len(lines):
        line = lines[i]
        if line.lstrip().startswith('```'):
            fence = not fence
            res.append(line)
            i += 1
            continue
        if ((not fence or in_code_too) and line.lstrip().startswith('|') and i + 1 < len(lines)
                and lines[i + 1].lstrip().startswith('|')
                and all(SEP.match(x) for x in split_row(lines[i + 1]))):
            j = i
            while j < len(lines) and lines[j].lstrip().startswith('|'):
                j += 1
            indent = line[:len(line) - len(line.lstrip())]
            res += [indent + r for r in align(lines[i:j])]
            i = j
            continue
        res.append(line)
        i += 1
    return '\n'.join(res)


if __name__ == '__main__':
    code = '--code' in sys.argv
    for path in [a for a in sys.argv[1:] if a != '--code']:
        with open(path, encoding='utf-8') as f:
            src = f.read()
        out = process(src, code)
        if out != src:
            with open(path, 'w', encoding='utf-8') as f:
                f.write(out)
            print('aligned', path)
