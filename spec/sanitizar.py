#!/usr/bin/env python3
"""Replaces possibly personal data in the specification examples.

Some examples copied from the public developer portal carry real-looking
CPFs, CNPJs, e-mail addresses, phone numbers and bank account numbers. This
script replaces them with synthetic values, keeping the rest of the document
byte for byte (the file is re-serialised with the same formatting). It is
idempotent; run it after every specification update:

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
SYNTHETIC_CNPJ = "12345678000195"
ALLOWED_CNPJS = {"11222333000181", "12345678000195"}
SYNTHETIC_PHONE = "+5500000000000"
ACCOUNT_DIGITS = "1234567890" * 3
# Domains reserved for documentation (RFC 2606 and RFC 6761): no one gets mail
# there. Addresses elsewhere, even made up, may reach someone.
RESERVED_DOMAINS = ("example.com", "example.net", "example.org")
RESERVED_TLDS = ("example", "test", "invalid", "localhost")

# Formatted or not, and not part of a longer code (an end-to-end id, a barcode).
CNPJ_IN_TEXT = re.compile(r"(?<![0-9A-Za-z])(\d{14}|\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2})(?![0-9A-Za-z])")
CPF_IN_TEXT = re.compile(r"(?<![0-9A-Za-z])(\d{3}\.\d{3}\.\d{3}-\d{2})(?![0-9A-Za-z])")
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+)")


def cpf_is_valid(digits: str) -> bool:
    if len(digits) != 11 or len(set(digits)) == 1:
        return False
    for size in (9, 10):
        total = sum(int(d) * (size + 1 - i) for i, d in enumerate(digits[:size]))
        if (total * 10) % 11 % 10 != int(digits[size]):
            return False
    return True


def cnpj_is_valid(digits: str) -> bool:
    if len(digits) != 14 or len(set(digits)) == 1:
        return False
    weights = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]
    for size in (12, 13):
        total = sum(int(d) * w for d, w in zip(digits[:size], weights[13 - size :]))
        remainder = total % 11
        if (0 if remainder < 2 else 11 - remainder) != int(digits[size]):
            return False
    return True


def is_reserved(domain: str) -> bool:
    domain = domain.lower()
    return any(domain == d or domain.endswith("." + d) for d in RESERVED_DOMAINS) or (
        domain.rsplit(".", 1)[-1] in RESERVED_TLDS
    )


def with_digits(template: str, digits: str) -> str:
    """`digits` in the punctuation of `template` (12.345.678/0001-95)."""
    replacement = iter(digits)
    return "".join(next(replacement) if c.isdigit() else c for c in template)


def sanitize_text(text: str) -> str:
    """CNPJs, formatted CPFs and e-mail addresses anywhere in the text,
    descriptions included."""

    def cnpj(match):
        found = match.group(1)
        digits = re.sub(r"\D", "", found)
        if cnpj_is_valid(digits) and digits not in ALLOWED_CNPJS:
            return with_digits(found, SYNTHETIC_CNPJ)
        return found

    def cpf(match):
        found = match.group(1)
        digits = re.sub(r"\D", "", found)
        if cpf_is_valid(digits) and digits not in ALLOWED_CPFS:
            return with_digits(found, SYNTHETIC_CPF)
        return found

    def email(match):
        if is_reserved(match.group(1)):
            return match.group(0)
        local = match.group(0).split("@", 1)[0]
        return f"{local}@example.com"

    text = CNPJ_IN_TEXT.sub(cnpj, text)
    text = CPF_IN_TEXT.sub(cpf, text)
    return EMAIL.sub(email, text)


def looks_like_cpf(text: str) -> bool:
    return re.fullmatch(r"\d{3}\.?\d{3}\.?\d{3}-?\d{2}", text) is not None


def sanitize(value, path):
    if isinstance(value, dict):
        return {key: sanitize(item, path + [key]) for key, item in value.items()}
    if isinstance(value, list):
        return [sanitize(item, path) for item in value]
    if isinstance(value, str):
        value = sanitize_text(value)
        in_cpf_field = any("cpf" in p.lower() for p in path)
        digits = re.sub(r"\D", "", value)
        if in_cpf_field and looks_like_cpf(value) and cpf_is_valid(digits) and digits not in ALLOWED_CPFS:
            return "123.456.789-09" if "." in value else SYNTHETIC_CPF
        if re.fullmatch(r"\+55\d{10,11}", value):
            return SYNTHETIC_PHONE
        if is_account_field(path) and re.fullmatch(r"\d{4,}", value) and set(value) != {"0"}:
            return ACCOUNT_DIGITS[: len(value)]
    if isinstance(value, int) and not isinstance(value, bool):
        digits = str(value)
        if is_account_field(path) and len(digits) >= 4 and set(digits) != {"0"}:
            return int(ACCOUNT_DIGITS[: len(digits)])
        if cnpj_is_valid(digits) and digits not in ALLOWED_CNPJS:
            return int(SYNTHETIC_CNPJ)
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
