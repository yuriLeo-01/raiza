//! Definição da linha de comando (clap) e conversão para [`CapabilityValue`].
//!
//! Nada aqui faz I/O: dá para testar só com `Cli::try_parse_from`.

use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use raiza_core::{CapabilityValue, Color, DeviceId, RgbSetting};

/// Hub de periféricos para Linux: fala com o daemon `raizad` pelo D-Bus.
#[derive(Debug, Parser)]
#[command(name = "raiza-ctl", version, about)]
pub struct Cli {
    /// O que fazer.
    #[command(subcommand)]
    pub command: Command,
}

/// Subcomandos de nível superior.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Lista os dispositivos conhecidos pelo daemon
    List {
        /// Imprime o JSON do daemon, para scripts (sempre com todos os aparelhos)
        #[arg(long)]
        json: bool,
        /// Mostra também os aparelhos sem controles (que só dizem "conectado")
        #[arg(long)]
        all: bool,
    },
    /// Mostra todas as capacidades de um dispositivo
    Get {
        /// Id do dispositivo (ex.: usb:24ae:1423:0)
        id: DeviceId,
        /// Imprime o JSON do daemon, para scripts
        #[arg(long)]
        json: bool,
    },
    /// Altera uma capacidade de um dispositivo
    Set {
        /// Id do dispositivo (ex.: usb:24ae:1423:0)
        id: DeviceId,
        /// Capacidade e novo valor
        #[command(subcommand)]
        value: SetCommand,
    },
    /// Acompanha os eventos do daemon em tempo real (um por linha)
    Watch {
        /// Imprime um objeto JSON por linha
        #[arg(long)]
        json: bool,
    },
    /// Gera o script de autocompletar para o seu shell
    Completions {
        /// Shell de destino
        shell: CompletionShell,
    },
}

/// Shells aceitos em `completions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    /// Bash
    Bash,
    /// Zsh
    Zsh,
    /// Fish
    Fish,
}

/// `on` ou `off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OnOff {
    /// Ligado
    On,
    /// Desligado
    Off,
}

impl OnOff {
    fn as_bool(self) -> bool {
        self == OnOff::On
    }
}

/// Uma capacidade e o valor novo, como se escreve no `set`.
#[derive(Debug, Subcommand)]
pub enum SetCommand {
    /// Resolução do sensor (DPI)
    Dpi {
        /// Novo DPI
        value: u32,
    },
    /// Um estágio da tabela de DPI (o em uso ou outro)
    DpiStage {
        /// Índice do estágio, a partir de 0
        stage: u32,
        /// Novo DPI do estágio
        value: u32,
    },
    /// Taxa de atualização em Hz
    PollingRate {
        /// Nova taxa em Hz
        value: u32,
    },
    /// Taxa de varredura (Key Scan Rate) em Hz
    ScanRate {
        /// Nova taxa em Hz
        value: u32,
    },
    /// Volume em porcentagem
    Volume {
        /// Novo volume (%)
        value: u32,
    },
    /// Liga ou desliga o mudo
    Mute {
        /// `on` muda, `off` tira do mudo
        state: OnOff,
    },
    /// Conecta ou desconecta o dispositivo
    Connection {
        /// `on` conecta, `off` desconecta
        state: OnOff,
    },
    /// Iluminação RGB (informe pelo menos --mode ou --color)
    #[command(group(
        ArgGroup::new("alteracao").required(true).multiple(true).args(["mode", "color"])
    ))]
    Rgb {
        /// Índice da zona; sem ele, vale para todas
        #[arg(long)]
        zone: Option<u32>,
        /// Nome do modo (ex.: static)
        #[arg(long)]
        mode: Option<String>,
        /// Cor no formato #rrggbb
        #[arg(long, value_name = "#RRGGBB")]
        color: Option<Color>,
    },
    /// Dispara um pulso de vibração
    Rumble {
        /// Intensidade de 0 a 100
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        strength: u8,
        /// Duração em milissegundos
        duration_ms: u32,
    },
}

impl SetCommand {
    /// Converte o que foi digitado no valor que o daemon espera.
    ///
    /// Só falha no `rgb` sem `--mode` nem `--color` (o clap já impede isso na linha de
    /// comando; a checagem aqui protege quem monta o comando à mão).
    pub fn to_value(&self) -> Result<CapabilityValue, String> {
        Ok(match self {
            SetCommand::Dpi { value } => CapabilityValue::Dpi(*value),
            SetCommand::DpiStage { stage, value } => CapabilityValue::DpiStage {
                stage: *stage,
                value: *value,
            },
            SetCommand::PollingRate { value } => CapabilityValue::PollingRate(*value),
            SetCommand::ScanRate { value } => CapabilityValue::ScanRate(*value),
            SetCommand::Volume { value } => CapabilityValue::Volume(*value),
            SetCommand::Mute { state } => CapabilityValue::Mute(state.as_bool()),
            SetCommand::Connection { state } => CapabilityValue::Connection(state.as_bool()),
            SetCommand::Rgb { zone, mode, color } => {
                if mode.is_none() && color.is_none() {
                    return Err("rgb: informe pelo menos --mode ou --color".into());
                }
                CapabilityValue::Rgb(RgbSetting {
                    zone: *zone,
                    mode: mode.clone(),
                    color: *color,
                })
            }
            SetCommand::Rumble {
                strength,
                duration_ms,
            } => CapabilityValue::Rumble {
                strength: *strength,
                duration_ms: *duration_ms,
            },
        })
    }
}

/// Escreve o script de autocompletar de `shell` em `out`.
pub fn completions(shell: CompletionShell, out: &mut dyn std::io::Write) -> std::io::Result<()> {
    use clap::CommandFactory;
    use clap_complete::{Shell, generate};

    let shell = match shell {
        CompletionShell::Bash => Shell::Bash,
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
    };
    // Gera num buffer: o `generate` entra em pânico se o pipe fechar no meio da escrita.
    let mut buf = Vec::new();
    generate(shell, &mut Cli::command(), "raiza-ctl", &mut buf);
    out.write_all(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "usb:24ae:1423:0";

    /// Converte `set <ID> <args…>` em valor, ou devolve o erro do clap.
    fn set(args: &[&str]) -> Result<CapabilityValue, String> {
        let mut argv = vec!["raiza-ctl", "set", ID];
        argv.extend_from_slice(args);
        let cli = Cli::try_parse_from(argv).map_err(|e| e.to_string())?;
        match cli.command {
            Command::Set { id, value } => {
                assert_eq!(id.to_string(), ID);
                value.to_value()
            }
            outro => panic!("esperava set, veio {outro:?}"),
        }
    }

    #[test]
    fn set_numericos() {
        assert_eq!(set(&["dpi", "1600"]), Ok(CapabilityValue::Dpi(1600)));
        assert_eq!(
            set(&["polling-rate", "1000"]),
            Ok(CapabilityValue::PollingRate(1000))
        );
        assert_eq!(set(&["volume", "40"]), Ok(CapabilityValue::Volume(40)));
        assert_eq!(
            set(&["dpi-stage", "2", "950"]),
            Ok(CapabilityValue::DpiStage {
                stage: 2,
                value: 950
            })
        );
        assert_eq!(
            set(&["scan-rate", "4000"]),
            Ok(CapabilityValue::ScanRate(4000))
        );
    }

    #[test]
    fn set_liga_desliga() {
        assert_eq!(set(&["mute", "on"]), Ok(CapabilityValue::Mute(true)));
        assert_eq!(set(&["mute", "off"]), Ok(CapabilityValue::Mute(false)));
        assert_eq!(
            set(&["connection", "on"]),
            Ok(CapabilityValue::Connection(true))
        );
        assert_eq!(
            set(&["connection", "off"]),
            Ok(CapabilityValue::Connection(false))
        );
    }

    #[test]
    fn set_rgb() {
        let cor = Color { r: 0, g: 255, b: 0 };
        assert_eq!(
            set(&[
                "rgb", "--zone", "1", "--mode", "static", "--color", "#00ff00"
            ]),
            Ok(CapabilityValue::Rgb(RgbSetting {
                zone: Some(1),
                mode: Some("static".into()),
                color: Some(cor),
            }))
        );
        assert_eq!(
            set(&["rgb", "--color", "#00ff00"]),
            Ok(CapabilityValue::Rgb(RgbSetting {
                zone: None,
                mode: None,
                color: Some(cor),
            }))
        );
        assert_eq!(
            set(&["rgb", "--mode", "wave"]),
            Ok(CapabilityValue::Rgb(RgbSetting {
                zone: None,
                mode: Some("wave".into()),
                color: None,
            }))
        );
    }

    #[test]
    fn set_rumble() {
        assert_eq!(
            set(&["rumble", "80", "500"]),
            Ok(CapabilityValue::Rumble {
                strength: 80,
                duration_ms: 500
            })
        );
    }

    #[test]
    fn set_invalidos() {
        // Falta o valor, valor que não é número, negativo.
        assert!(set(&["dpi"]).is_err());
        assert!(set(&["dpi", "alto"]).is_err());
        assert!(set(&["volume", "-5"]).is_err());
        assert!(set(&["polling-rate", "1000", "2000"]).is_err());
        // Capacidade desconhecida (inclusive a forma com sublinhado).
        assert!(set(&["brilho", "3"]).is_err());
        assert!(set(&["polling_rate", "1000"]).is_err());
        // on/off.
        assert!(set(&["mute", "talvez"]).is_err());
        assert!(set(&["mute"]).is_err());
        assert!(set(&["connection", "1"]).is_err());
        // rgb: nada informado, cor ruim, zona ruim.
        assert!(set(&["rgb"]).is_err());
        assert!(set(&["rgb", "--zone", "0"]).is_err());
        assert!(set(&["rgb", "--color", "verde"]).is_err());
        assert!(set(&["rgb", "--color", "00ff00"]).is_err());
        assert!(set(&["rgb", "--zone", "x", "--mode", "static"]).is_err());
        // rumble: intensidade acima de 100, argumentos faltando.
        assert!(set(&["rumble", "101", "500"]).is_err());
        assert!(set(&["rumble", "50"]).is_err());
        assert!(set(&["rumble", "50", "-1"]).is_err());
    }

    #[test]
    fn id_invalido() {
        assert!(Cli::try_parse_from(["raiza-ctl", "set", "lixo", "dpi", "800"]).is_err());
        assert!(Cli::try_parse_from(["raiza-ctl", "get", "lixo"]).is_err());
        assert!(Cli::try_parse_from(["raiza-ctl", "get"]).is_err());
    }

    #[test]
    fn rgb_sem_nada_na_mao_e_recusado() {
        let vazio = SetCommand::Rgb {
            zone: Some(0),
            mode: None,
            color: None,
        };
        assert!(vazio.to_value().is_err());
    }

    #[test]
    fn outros_subcomandos() {
        let json = |args: &[&str]| {
            let mut v = vec!["raiza-ctl"];
            v.extend_from_slice(args);
            match Cli::try_parse_from(v).unwrap().command {
                Command::List { json, .. }
                | Command::Watch { json }
                | Command::Get { json, .. } => json,
                _ => panic!("subcomando inesperado"),
            }
        };
        assert!(!json(&["list"]));
        assert!(json(&["list", "--json"]));
        assert!(json(&["get", ID, "--json"]));
        assert!(json(&["watch", "--json"]));
        assert!(Cli::try_parse_from(["raiza-ctl", "completions", "fish"]).is_ok());
        assert!(Cli::try_parse_from(["raiza-ctl", "completions", "powershell"]).is_err());
        assert!(Cli::try_parse_from(["raiza-ctl"]).is_err());
    }

    #[test]
    fn completions_geram_script() {
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
        ] {
            let mut buf = Vec::new();
            completions(shell, &mut buf).unwrap();
            assert!(String::from_utf8(buf).unwrap().contains("raiza-ctl"));
        }
    }
}
