use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use super::{
    ControlRole, CurveRef, MixerOutput, ModelConfig, OutputProtocol, RadioConfig, RateProfile,
};

#[cfg(test)]
pub(crate) static TEST_CWD_MUTEX: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

pub const RADIO_CONFIG_PATH: &str = "radio.toml";
pub const MODELS_DIR: &str = "models";

pub fn ensure_default_layout() -> io::Result<()> {
    fs::create_dir_all(MODELS_DIR)?;

    if !Path::new(RADIO_CONFIG_PATH).exists() {
        save_radio_config(&RadioConfig::default())?;
    }

    if list_model_paths()?.is_empty() {
        for model in sample_models() {
            save_model_config(&model)?;
        }
    }

    let radio = load_radio_config()?;
    if load_model_config(&radio.active_model).is_err() {
        let fallback_id = if load_model_config("quad_x").is_ok() {
            "quad_x".to_string()
        } else if let Some(first) = list_models()?.into_iter().next() {
            first.id
        } else {
            radio.active_model
        };
        if !fallback_id.is_empty() {
            let _ = set_active_model(&fallback_id)?;
        }
    }

    Ok(())
}

pub fn load_radio_config() -> io::Result<RadioConfig> {
    let content = fs::read_to_string(RADIO_CONFIG_PATH)?;
    toml::from_str(&content).map_err(|err| io::Error::new(ErrorKind::InvalidData, err))
}

pub fn save_radio_config(config: &RadioConfig) -> io::Result<()> {
    let content = toml::to_string_pretty(config)
        .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?;
    fs::write(RADIO_CONFIG_PATH, content)
}

pub fn list_models() -> io::Result<Vec<ModelConfig>> {
    let mut models = Vec::new();
    for path in list_model_paths()? {
        let content = fs::read_to_string(path)?;
        let model = toml::from_str::<ModelConfig>(&content)
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?;
        models.push(model);
    }
    models.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    Ok(models)
}

pub fn load_model_config(id: &str) -> io::Result<ModelConfig> {
    let path = model_path(id);
    let content = fs::read_to_string(path)?;
    toml::from_str(&content).map_err(|err| io::Error::new(ErrorKind::InvalidData, err))
}

pub fn save_model_config(config: &ModelConfig) -> io::Result<()> {
    fs::create_dir_all(MODELS_DIR)?;
    let content = toml::to_string_pretty(config)
        .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?;
    fs::write(model_path(&config.id), content)
}

pub fn load_active_model() -> io::Result<ModelConfig> {
    let radio = load_radio_config()?;
    load_model_config(&radio.active_model)
}

pub fn set_active_model(id: &str) -> io::Result<ModelConfig> {
    let model = load_model_config(id)?;
    let mut radio = load_radio_config()?;
    radio.active_model = model.id.clone();
    save_radio_config(&radio)?;
    Ok(model)
}

fn list_model_paths() -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if !Path::new(MODELS_DIR).exists() {
        return Ok(paths);
    }

    for entry in fs::read_dir(MODELS_DIR)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn model_path(id: &str) -> PathBuf {
    Path::new(MODELS_DIR).join(format!("{}.toml", sanitize_id(id)))
}

fn sanitize_id(id: &str) -> String {
    let sanitized: String = id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

fn sample_models() -> Vec<ModelConfig> {
    vec![sample_quad(), sample_plane(), sample_rover()]
}

fn sample_quad() -> ModelConfig {
    let mut model = ModelConfig::default();
    model.id = "quad_x".to_string();
    model.name = "Quad X".to_string();
    model.output.protocol = OutputProtocol::Crsf;
    model.profiles = vec![RateProfile {
        name: "acro".to_string(),
        roll_rate: 220,
        pitch_rate: 220,
        yaw_rate: 180,
        expo_percent: 20,
    }];
    model
}

fn sample_plane() -> ModelConfig {
    let mut model = ModelConfig::default();
    model.id = "fixed_wing".to_string();
    model.name = "Fixed Wing".to_string();
    model.output.protocol = OutputProtocol::Crsf;
    for output in &mut model.mixer.outputs {
        if output.role == ControlRole::Elevator {
            output.limits.reversed = true;
        }
        if output.role == ControlRole::Thrust {
            output.weight = 80;
        }
    }
    model.profiles = vec![RateProfile {
        name: "cruise".to_string(),
        roll_rate: 120,
        pitch_rate: 100,
        yaw_rate: 80,
        expo_percent: 10,
    }];
    model
}

fn sample_rover() -> ModelConfig {
    let mut model = ModelConfig::default();
    model.id = "rover".to_string();
    model.name = "Rover".to_string();
    model.output.protocol = OutputProtocol::UsbHid;
    model.mixer.outputs = vec![
        MixerOutput {
            role: ControlRole::Thrust,
            weight: 60,
            offset: -150,
            curve: CurveRef::Linear,
            limits: super::OutputLimits::default(),
        },
        MixerOutput {
            role: ControlRole::Direction,
            weight: 140,
            offset: 0,
            curve: CurveRef::Linear,
            limits: super::OutputLimits::default(),
        },
        MixerOutput::new(ControlRole::Aileron),
        MixerOutput::new(ControlRole::Elevator),
    ];
    model.profiles = vec![RateProfile {
        name: "ground".to_string(),
        roll_rate: 60,
        pitch_rate: 60,
        yaw_rate: 90,
        expo_percent: 0,
    }];
    model
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct TestCwdGuard {
        original: PathBuf,
        test_dir: PathBuf,
    }

    impl TestCwdGuard {
        fn new() -> Self {
            let original = std::env::current_dir().unwrap();
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let test_dir = std::env::temp_dir().join(format!("lintx-config-test-{unique}"));
            fs::create_dir_all(&test_dir).unwrap();
            std::env::set_current_dir(&test_dir).unwrap();
            Self { original, test_dir }
        }
    }

    impl Drop for TestCwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
            let _ = fs::remove_dir_all(&self.test_dir);
        }
    }

    #[test]
    fn test_ensure_default_layout_creates_models_and_radio() {
        let _serial = TEST_CWD_MUTEX.lock().unwrap();
        let _guard = TestCwdGuard::new();
        ensure_default_layout().unwrap();
        assert!(Path::new(RADIO_CONFIG_PATH).exists());
        assert!(Path::new(MODELS_DIR).exists());
        assert!(list_models().unwrap().len() >= 3);
        assert_eq!(load_active_model().unwrap().id, "quad_x");
    }

    #[test]
    fn test_set_active_model_updates_radio_config() {
        let _serial = TEST_CWD_MUTEX.lock().unwrap();
        let _guard = TestCwdGuard::new();
        ensure_default_layout().unwrap();
        let model = set_active_model("rover").unwrap();
        assert_eq!(model.id, "rover");
        assert_eq!(load_radio_config().unwrap().active_model, "rover");
    }
}
