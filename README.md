# VideoNova

<div align="center">
  <img src="docs/images/app-logo.png" alt="VideoNova Logo" width="200"/>
  <h3>Автоматизированный переводчик и дубляж для видео</h3>
</div>

## 📋 Описание

VideoNova - это кроссплатформенное десктопное приложение для автоматического перевода и озвучивания видеоконтента. Приложение позволяет быстро и качественно создавать локализованные версии видео путем перевода субтитров и генерации синтезированной речи.

## ✨ Возможности

- **Загрузка видео**: Скачивание видео с YouTube и других платформ с помощью yt-dlp
- **Распознавание речи**: Автоматическое распознавание речи и создание субтитров с помощью Whisper
- **Перевод субтитров**: Высококачественный перевод субтитров с сохранением временных меток
- **Синтез речи**: Создание озвучки переведенного текста с использованием TTS сервисов
- **Объединение компонентов**: Интеграция переведенной аудиодорожки и субтитров в исходное видео
- **Автоматические обновления**: Встроенная система проверки и обновления зависимостей

## 🖥️ Скриншоты

[Место для скриншотов приложения]

## 🚀 Установка

### Предварительно собранные бинарные файлы

1. Перейдите на [страницу релизов](https://github.com/region23/videonova/releases)
2. Загрузите версию для вашей операционной системы:
   - macOS: `VideoNova_x.x.x_macos.dmg`
   - Windows: `VideoNova_x.x.x_windows_setup.exe`
   - Linux: `videonova_x.x.x_amd64.deb` или `videonova_x.x.x_amd64.AppImage`

### Сборка из исходного кода

#### Системные требования

- [Node.js](https://nodejs.org/) (версия 18 или выше)
- [Rust](https://www.rust-lang.org/) (последняя стабильная версия)
- [pnpm](https://pnpm.io/) (версия 8 или выше)

#### Шаги сборки

```bash
# Клонирование репозитория
git clone https://github.com/region23/videonova.git
cd videonova

# Установка зависимостей
pnpm install

# Разработка
pnpm tauri dev

# Сборка
pnpm tauri build
```

## 🛠️ Использование

1. Запустите приложение VideoNova
2. Введите URL видео для скачивания или выберите локальный файл
3. Выберите исходный и целевой языки
4. Выберите папку для сохранения результата
5. Нажмите кнопку "Старт" и дождитесь завершения обработки
6. Готовое видео с переводом будет сохранено в указанной папке

## 🤝 Участие в разработке

Мы приветствуем вклад в развитие проекта! Если вы хотите принять участие, пожалуйста, ознакомьтесь с нашим [руководством по участию](CONTRIBUTING.md).

### Настройка CI/CD для разработчиков

В проекте настроена система непрерывной интеграции и доставки на GitHub Actions. Вот как настроить её для работы с вашей копией репозитория:

#### 1. Включить GitHub Actions в репозитории

1. Перейдите в ваш репозиторий на GitHub
2. Перейдите в раздел "Settings" (Настройки) репозитория
3. В боковом меню выберите "Actions" → "General"
4. Убедитесь, что выбран пункт "Allow all actions and reusable workflows" или, как минимум, разрешены необходимые действия

#### 2. Настроить разрешения для GITHUB_TOKEN

1. В тех же настройках репозитория ("Settings")
2. Прокрутите вниз до раздела "Workflow permissions"
3. Выберите "Read and write permissions" (это даст возможность рабочим процессам создавать релизы)
4. Сохраните настройки

#### 3. Создать ветку main (если её нет)

```bash
git checkout -b main
git push -u origin main
```

#### 4. Защита ветки main (опционально, но рекомендуется)

1. В настройках репозитория перейдите в "Branches"
2. Выберите "Add branch protection rule"
3. Укажите pattern `main`
4. Включите опции:
   - "Require a pull request before merging"
   - "Require status checks to pass before merging" и в поиске выберите "check-build" (наш CI workflow)
   - "Require branches to be up to date before merging"

#### 5. Управление версиями

Для управления версиями используйте workflow "Bump Version":

1. Перейдите в раздел "Actions" вашего репозитория
2. Выберите "Bump Version" из списка workflows
3. Нажмите "Run workflow"
4. Выберите тип изменения версии (patch, minor или major)
5. Нажмите "Run workflow"

#### 6. Публикация релизов

После того как версия обновлена и новый код попал в ветку main:

1. GitHub Actions автоматически создаст черновик релиза и соберет билды
2. Перейдите в раздел "Releases" вашего репозитория
3. Найдите созданный черновик релиза
4. Проверьте автоматически сгенерированные release notes и загруженные артефакты
5. При необходимости внесите правки в описание релиза
6. Нажмите "Publish release" для публикации

## 📝 Дополнительные рекомендации по работе с CI/CD

1. **Использование labels для PR и issues**: Правильно маркируйте PR и issues метками (labels) согласно их типу (bug, enhancement, documentation и т.д.), это поможет автоматически группировать изменения в release notes.

2. **Связывание PR с issues**: В описании PR используйте ключевые слова "Fixes #X" или "Resolves #X", где X - номер issue. Это позволит автоматически закрывать связанные issues при слиянии PR и улучшит структуру release notes.

3. **Следование шаблонам**: Используйте созданные шаблоны для PR и issues, чтобы предоставлять всю необходимую информацию, что также улучшит качество release notes.

## 📃 Документация API

Полная документация по API и внутренним компонентам доступна в [документации](docs/README.md).

## 📄 Лицензия

Этот проект распространяется под лицензией [MIT](LICENSE).

## ❤️ Благодарности

- [Tauri](https://tauri.app/) - фреймворк для создания кроссплатформенных приложений
- [Vue.js](https://vuejs.org/) - фронтенд-фреймворк
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) - загрузчик видео
- [Whisper](https://openai.com/research/whisper) - система распознавания речи от OpenAI
- [FFmpeg](https://ffmpeg.org/) - набор библиотек и программ для обработки мультимедиа
- [OpenAI](https://openai.com/) - API для перевода и синтеза речи
