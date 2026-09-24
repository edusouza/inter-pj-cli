#!/usr/bin/env python3
"""Replaces possibly personal data in the specification examples.

Some examples copied from the public developer portal carry real-looking
CPFs, phone numbers and bank account numbers. This script replaces them with
synthetic values, keeping the rest of the document byte for byte (the file is
re-serialised with the same formatting). It is idempotent; run it after every
specification update:

    python3 spec/sanitizar.py

The contract test `spec_examples_contain_no_real_looking_personal_data`
(crates/inter-pj/tests/contract.rs) fails if anything slips through.
"""

import json
import re
from pathlib import Path

SPEC = Path(__file__).with_name("inter-empresas-openapi.json")

SYNTHETIC_CPF = "12345678909"
# Sequences that are obviously not real people's CPFs (valid check digits by chance).
ALLOWED_CPFS = {"01234567890", "12345678909"}
SYNTHETIC_PHONE = "+5500000000000"
ACCOUNT_DIGITS = "1234567890" * 3


def cpf_is_valid(digits: str) -> bool:
    if len(digits) != 11 or len(set(digits)) == 1:
        return False
    for size in (9, 10):
        total = sum(int(d) * (size + 1 - i) for i, d in enumerate(digits[:size]))
        if (total * 10) % 11 % 10 != int(digits[size]):
            return False
    return True


def looks_like_cpf(text: str) -> bool:
    return re.fullmatch(r"\d{3}\.?\d{3}\.?\d{3}-?\d{2}", text) is not None


def sanitize(value, path):
    if isinstance(value, dict):
        return {key: sanitize(item, path + [key]) for key, item in value.items()}
    if isinstance(value, list):
        return [sanitize(item, path) for item in value]
    if isinstance(value, str):
        in_cpf_field = any("cpf" in p.lower() for p in path)
        digits = re.sub(r"\D", "", value)
        if in_cpf_field and looks_like_cpf(value) and cpf_is_valid(digits) and digits not in ALLOWED_CPFS:
            return "123.456.789-09" if "." in value else SYNTHETIC_CPF
        if re.fullmatch(r"\+55\d{10,11}", value):
            return SYNTHETIC_PHONE
        if is_account_field(path) and re.fullmatch(r"\d{4,}", value) and set(value) != {"0"}:
            return ACCOUNT_DIGITS[: len(value)]
    if isinstance(value, int) and not isinstance(value, bool) and is_account_field(path):
        digits = str(value)
        if len(digits) >= 4 and set(digits) != {"0"}:
            return int(ACCOUNT_DIGITS[: len(digits)])
    return value


def is_account_field(path) -> bool:
    names = [p for p in path if p not in ("example", "value", "default")]
    return bool(names) and "conta" in names[-1].lower()


def main() -> None:
    raw = SPEC.read_text(encoding="utf-8")
    spec = json.loads(raw)
    if json.dumps(spec, indent=2, ensure_ascii=False) != raw:
        raise SystemExit("formatação inesperada: o arquivo não seria preservado byte a byte")
    sanitized = json.dumps(sanitize(spec, []), indent=2, ensure_ascii=False)
    if sanitized != raw:
        SPEC.write_text(sanitized, encoding="utf-8")
        print(f"{SPEC.name}: exemplos sanitizados")
    else:
        print(f"{SPEC.name}: nada a sanitizar")


if __name__ == "__main__":
    main()
