//! QR Code of a Pix copia e cola, drawn in the terminal or saved as PNG.

use std::io::{self, IsTerminal};

use inter_pj::pix::BrCode;
use qrcode::{Color, EcLevel, QrCode};

/// Light modules around the code, as the standard asks, so phones find it.
const MARGEM: usize = 4;

/// Pixels per module in the PNG.
pub(crate) const ESCALA_PNG: usize = 8;

/// Whether to paint the QR Code with ANSI colors: the standard output is a
/// terminal and `NO_COLOR` is not set (<https://no-color.org>).
pub(crate) fn cores() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|valor| valor.is_empty())
}

/// The modules of a QR Code, with the margin.
pub(crate) struct QrPix {
    /// Modules per side, margin included.
    lado: usize,
    /// Row by row; `true` is dark.
    escuros: Vec<bool>,
}

impl QrPix {
    /// Encodes a copia e cola, after checking it is a well-formed Pix BR
    /// Code with a correct CRC: a damaged code would draw a QR Code that
    /// no bank accepts.
    pub(crate) fn new(copia_e_cola: &str) -> Result<Self, String> {
        let copia_e_cola = copia_e_cola.trim();
        BrCode::parse(copia_e_cola).map_err(|err| format!("Pix copia e cola inválido: {err}"))?;
        let codigo = QrCode::with_error_correction_level(copia_e_cola, EcLevel::M)
            .map_err(|err| format!("não foi possível gerar o QR Code: {err}"))?;
        let largura = codigo.width();
        let cores = codigo.into_colors();
        let lado = largura + 2 * MARGEM;
        let mut escuros = vec![false; lado * lado];
        for y in 0..largura {
            for x in 0..largura {
                escuros[(y + MARGEM) * lado + x + MARGEM] = cores[y * largura + x] == Color::Dark;
            }
        }
        Ok(Self { lado, escuros })
    }

    fn escuro(&self, x: usize, y: usize) -> bool {
        x < self.lado && y < self.lado && self.escuros[y * self.lado + x]
    }

    /// Two rows of modules per line of text, with half blocks.
    ///
    /// With `cores`, ANSI sequences paint the code black on white, whatever
    /// the terminal's theme. Without them (`NO_COLOR`, output redirected),
    /// the light modules are the ones drawn, as `qrencode -t UTF8` does: the
    /// code reads right on terminals with a dark background.
    pub(crate) fn terminal(&self, cores: bool) -> String {
        let mut texto = String::new();
        for y in (0..self.lado).step_by(2) {
            if cores {
                texto.push_str("\u{1b}[30;107m");
            }
            for x in 0..self.lado {
                let (cima, baixo) = (self.escuro(x, y), self.escuro(x, y + 1));
                // Drawn: the dark modules in color, the light ones otherwise.
                let (cima, baixo) = if cores {
                    (cima, baixo)
                } else {
                    (!cima, y + 1 < self.lado && !baixo)
                };
                texto.push(match (cima, baixo) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => ' ',
                });
            }
            if cores {
                texto.push_str("\u{1b}[0m");
            }
            texto.push('\n');
        }
        texto
    }

    /// A black and white PNG, [`ESCALA_PNG`] pixels per module.
    pub(crate) fn png(&self) -> Vec<u8> {
        let lado = self.lado * ESCALA_PNG;
        // One bit per pixel (1 is white), rows starting with filter 0.
        let bytes_por_linha = lado.div_ceil(8);
        let mut pixels = Vec::with_capacity(lado * (bytes_por_linha + 1));
        for y in 0..lado {
            pixels.push(0);
            let mut linha = vec![0u8; bytes_por_linha];
            for x in 0..lado {
                if !self.escuro(x / ESCALA_PNG, y / ESCALA_PNG) {
                    linha[x / 8] |= 0x80 >> (x % 8);
                }
            }
            pixels.extend_from_slice(&linha);
        }
        png::escrever(lado, &pixels)
    }
}

/// Just enough PNG: grayscale of 1 bit, the image in stored (uncompressed)
/// deflate blocks, which the format allows.
mod png {
    const ASSINATURA: &[u8] = b"\x89PNG\r\n\x1a\n";
    /// Largest stored deflate block.
    const BLOCO: usize = 65_535;

    pub(super) fn escrever(lado: usize, pixels: &[u8]) -> Vec<u8> {
        let lado = u32::try_from(lado).expect("QR Code maior que um PNG");
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&lado.to_be_bytes());
        ihdr.extend_from_slice(&lado.to_be_bytes());
        // 1 bit, grayscale, deflate, filters per line, not interlaced.
        ihdr.extend_from_slice(&[1, 0, 0, 0, 0]);

        let mut png = ASSINATURA.to_vec();
        chunk(&mut png, *b"IHDR", &ihdr);
        chunk(&mut png, *b"IDAT", &zlib(pixels));
        chunk(&mut png, *b"IEND", &[]);
        png
    }

    fn chunk(png: &mut Vec<u8>, tipo: [u8; 4], dados: &[u8]) {
        let tamanho = u32::try_from(dados.len()).expect("chunk PNG grande demais");
        png.extend_from_slice(&tamanho.to_be_bytes());
        let inicio = png.len();
        png.extend_from_slice(&tipo);
        png.extend_from_slice(dados);
        let crc = crc32(&png[inicio..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }

    pub(super) fn zlib(dados: &[u8]) -> Vec<u8> {
        // Deflate without compression, 32 KiB window.
        let mut saida = vec![0x78, 0x01];
        let blocos: Vec<&[u8]> = dados.chunks(BLOCO).collect();
        for (i, bloco) in blocos.iter().enumerate() {
            let ultimo = u8::from(i + 1 == blocos.len());
            let tamanho = u16::try_from(bloco.len()).expect("bloco deflate de até 65535 bytes");
            saida.push(ultimo);
            saida.extend_from_slice(&tamanho.to_le_bytes());
            saida.extend_from_slice(&(!tamanho).to_le_bytes());
            saida.extend_from_slice(bloco);
        }
        if blocos.is_empty() {
            saida.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
        }
        saida.extend_from_slice(&adler32(dados).to_be_bytes());
        saida
    }

    pub(super) fn crc32(dados: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in dados {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    pub(super) fn adler32(dados: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in dados {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        (b << 16) | a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Example of the Banco Central's manual.
    const MANUAL: &str = "00020126580014br.gov.bcb.pix0136123e4567-e12b-12d1-a456-4266554400005204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***63041D3D";

    /// Reads a QR Code from a grid of modules: what a phone does.
    fn ler(lado: usize, escuro: impl Fn(usize, usize) -> bool) -> String {
        const PX: usize = 4;
        let mut imagem =
            rqrr::PreparedImage::prepare_from_greyscale(lado * PX, lado * PX, |x, y| {
                if escuro(x / PX, y / PX) { 0 } else { 255 }
            });
        let grades = imagem.detect_grids();
        assert_eq!(grades.len(), 1, "QR Code não encontrado");
        grades[0].decode().unwrap().1
    }

    /// The modules back from the text drawn in the terminal.
    fn modulos_do_terminal(texto: &str, cores: bool) -> Vec<Vec<bool>> {
        let sem_ansi = texto.replace("\u{1b}[30;107m", "").replace("\u{1b}[0m", "");
        let mut linhas = Vec::new();
        for linha in sem_ansi.lines() {
            let (mut cima, mut baixo) = (Vec::new(), Vec::new());
            for c in linha.chars() {
                let (a, b) = match c {
                    '█' => (true, true),
                    '▀' => (true, false),
                    '▄' => (false, true),
                    ' ' => (false, false),
                    outro => panic!("caractere inesperado {outro:?}"),
                };
                // Without colors, the drawn modules are the light ones.
                cima.push(a == cores);
                baixo.push(b == cores);
            }
            linhas.push(cima);
            linhas.push(baixo);
        }
        linhas
    }

    #[test]
    fn terminal_codes_read_back_as_the_copia_e_cola() {
        let qr = QrPix::new(MANUAL).unwrap();
        for cores in [true, false] {
            let texto = qr.terminal(cores);
            let modulos = modulos_do_terminal(&texto, cores);
            let lado = modulos[0].len();
            assert_eq!(lado, qr.lado);
            assert_eq!(ler(lado, |x, y| modulos[y][x]), MANUAL, "cores: {cores}");
        }
    }

    #[test]
    fn terminal_codes_have_the_quiet_zone() {
        let qr = QrPix::new(MANUAL).unwrap();
        let texto = qr.terminal(true);
        let linhas: Vec<&str> = texto.lines().collect();
        assert_eq!(linhas.len(), qr.lado.div_ceil(2));
        // Two lines of text are the four light modules above the code.
        for linha in &linhas[..2] {
            assert_eq!(
                linha,
                &format!("\u{1b}[30;107m{}\u{1b}[0m", " ".repeat(qr.lado))
            );
        }
        // Without colors, the margin is drawn, and nothing else is printed.
        let simples = qr.terminal(false);
        assert!(!simples.contains('\u{1b}'));
        assert!(simples.lines().next().unwrap().chars().all(|c| c == '█'));
    }

    /// Our PNG, read back with a decoder written from the format: the
    /// signature, the CRC of every chunk, the zlib stream and its checksum.
    fn ler_png(png: &[u8]) -> (usize, Vec<bool>) {
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let mut resto = &png[8..];
        let (mut lado, mut idat) = (0, Vec::new());
        while !resto.is_empty() {
            let tamanho =
                usize::try_from(u32::from_be_bytes(resto[..4].try_into().unwrap())).unwrap();
            let tipo = &resto[4..8];
            let dados = &resto[8..8 + tamanho];
            let crc = u32::from_be_bytes(resto[8 + tamanho..12 + tamanho].try_into().unwrap());
            assert_eq!(png::crc32(&resto[4..8 + tamanho]), crc);
            match tipo {
                b"IHDR" => {
                    lado = usize::try_from(u32::from_be_bytes(dados[..4].try_into().unwrap()))
                        .unwrap();
                    assert_eq!(&dados[8..], [1, 0, 0, 0, 0]);
                }
                b"IDAT" => idat.extend_from_slice(dados),
                _ => {}
            }
            resto = &resto[12 + tamanho..];
        }
        assert_eq!(&idat[..2], [0x78, 0x01]);
        let mut bruto = Vec::new();
        let mut pos = 2;
        loop {
            let ultimo = idat[pos] == 1;
            let tamanho = usize::from(u16::from_le_bytes([idat[pos + 1], idat[pos + 2]]));
            bruto.extend_from_slice(&idat[pos + 5..pos + 5 + tamanho]);
            pos += 5 + tamanho;
            if ultimo {
                break;
            }
        }
        let adler = u32::from_be_bytes(idat[pos..pos + 4].try_into().unwrap());
        assert_eq!(png::adler32(&bruto), adler);
        let bytes_por_linha = lado.div_ceil(8);
        let mut pixels = Vec::with_capacity(lado * lado);
        for linha in bruto.chunks(bytes_por_linha + 1) {
            assert_eq!(linha[0], 0, "filtro");
            for x in 0..lado {
                pixels.push(linha[1 + x / 8] & (0x80 >> (x % 8)) == 0);
            }
        }
        (lado, pixels)
    }

    #[test]
    fn png_codes_read_back_as_the_copia_e_cola() {
        let qr = QrPix::new(MANUAL).unwrap();
        let png = qr.png();
        let (lado, escuros) = ler_png(&png);
        assert_eq!(lado, qr.lado * ESCALA_PNG);
        // The decoder reads modules: one pixel of each.
        let modulos = lado / ESCALA_PNG;
        let conteudo = ler(modulos, |x, y| {
            escuros[(y * ESCALA_PNG + ESCALA_PNG / 2) * lado + x * ESCALA_PNG + ESCALA_PNG / 2]
        });
        assert_eq!(conteudo, MANUAL);
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(png::crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(png::adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn damaged_codes_are_not_drawn() {
        let danificado = MANUAL.replace("Fulano", "Fulana");
        let erro = QrPix::new(&danificado).err().unwrap();
        assert!(erro.starts_with("Pix copia e cola inválido: "), "{erro}");
        assert!(QrPix::new("texto qualquer").is_err());
    }

    #[test]
    fn long_codes_become_several_deflate_blocks() {
        // A large image still reads back through the stored blocks.
        let pixels = vec![0xAB; 3 * 65_535 + 10];
        let zlib = png::zlib(&pixels);
        assert_eq!(zlib.len(), 2 + 4 * 5 + pixels.len() + 4);
    }
}
