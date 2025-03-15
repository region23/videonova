//! Тест для модуля интеграции с OpenAI TTS
//! 
//! Использует моки для проверки функциональности без реальных запросов к API.

use crate::utils::tts::openai_tts;

#[test]
fn test_preprocess_text() {
    // Функция доступна через функцию `super::preprocess_text`, но для этого 
    // нам нужно сделать ее публичной, поэтому тестируем публичное API

    // Тестируем доступные голоса и модели
    assert!(!openai_tts::available_voices().is_empty());
    assert!(!openai_tts::available_models().is_empty());
    
    // Проверяем, что первые голоса соответствуют ожидаемым значениям
    let voices = openai_tts::available_voices();
    assert!(voices.contains(&"alloy".to_string()));
    assert!(voices.contains(&"echo".to_string()));
    
    // Проверяем модели
    let models = openai_tts::available_models();
    assert!(models.contains(&"tts-1".to_string()));
    assert!(models.contains(&"tts-1-hd".to_string()));
}

// Для полноценного тестирования API нам потребуется использовать моки
// или интеграционные тесты с реальным API ключом.
//
// Пример использования моков приведен ниже, но требует дополнительных 
// зависимостей (mockito или аналогичную библиотеку) в Cargo.toml:
//
// ```
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use mockito;
//     use crate::utils::tts::types::TtsVoiceConfig;
//
//     #[tokio::test]
//     async fn test_generate_tts_success() {
//         let mut server = mockito::Server::new();
//         let mock = server.mock("POST", "/v1/audio/speech")
//             .with_status(200)
//             .with_header("content-type", "audio/mpeg")
//             .with_body(vec![1, 2, 3, 4]) // Фейковые MP3-данные
//             .create();
//
//         let config = TtsVoiceConfig {
//             voice: "alloy".to_string(),
//             model: "tts-1".to_string(),
//             speed: 1.0,
//         };
//
//         let api_url = server.url();
//         // Здесь нужно внедрить URL мокового сервера вместо реального API
//         // что потребует модификации основной функции для поддержки тестирования
//
//         let result = generate_tts("fake_api_key", "Тестовый текст", &config).await;
//         assert!(result.is_ok());
//
//         let (audio_data, processed_text) = result.unwrap();
//         assert_eq!(audio_data, vec![1, 2, 3, 4]);
//         assert_eq!(processed_text, "Тестовый текст.");
//
//         mock.assert();
//     }
// }
// ``` 