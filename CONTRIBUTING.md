# Руководство по участию в разработке

Спасибо за интерес к проекту VideoNova! Мы рады любому вкладу, будь то исправление ошибок, добавление новых функций или улучшение документации.

## Работа с issues

### Сообщение об ошибке

Если вы нашли ошибку, пожалуйста, создайте issue в нашем трекере, используя шаблон для отчета об ошибке. Убедитесь, что предоставили следующую информацию:

- Четкое описание ошибки
- Шаги для воспроизведения
- Ожидаемое поведение
- Фактическое поведение
- Скриншоты (если возможно)
- Версия приложения и операционная система

### Предложение новой функциональности

Если у вас есть идея новой функции, создайте issue с шаблоном запроса новой функции. Опишите подробно:

- Какую проблему решает эта функция
- Как вы представляете реализацию
- Почему эта функция важна для проекта

## Порядок внесения изменений

### 1. Настройка окружения разработки

```bash
# Форк репозитория
# Клонирование вашего форка
git clone https://github.com/region23/videonova.git
cd videonova

# Установка зависимостей
pnpm install
```

### 2. Создание ветки для разработки

```bash
# Создание новой ветки для ваших изменений
git checkout -b feature/my-awesome-feature
# или
git checkout -b fix/bug-description
```

Следуйте этим соглашениям для именования веток:
- `feature/` - для новых функций
- `fix/` - для исправления ошибок
- `docs/` - для изменений в документации
- `refactor/` - для рефакторинга кода

### 3. Разработка

- Пишите тесты для новой функциональности (если применимо)
- Убедитесь, что код следует стилю проекта
- Комментируйте сложные моменты
- Проверьте, что ваши изменения не вызывают новых предупреждений линтера

### 4. Коммиты

Используйте семантические сообщения для коммитов:

```
feat: добавлена поддержка нового TTS провайдера
^     ^
|     +-> Описание изменений
|
+-------> Тип: feat, fix, docs, style, refactor, test, chore
```

Типы коммитов:
- `feat`: Новая функциональность
- `fix`: Исправление ошибки
- `docs`: Изменения в документации
- `style`: Форматирование, отступы, точки с запятой и т.д.
- `refactor`: Рефакторинг кода без изменения функциональности
- `test`: Добавление тестов
- `chore`: Обновление зависимостей, настройка сборки и т.д.

### 5. Отправка Pull Request

1. Обновите вашу ветку из основного репозитория:
   ```bash
   git remote add upstream https://github.com/region23/videonova.git
   git fetch upstream
   git rebase upstream/main
   ```

2. Отправьте изменения в ваш форк:
   ```bash
   git push origin feature/my-awesome-feature
   ```

3. Создайте Pull Request через интерфейс GitHub

4. В описании PR используйте предоставленный шаблон, указав:
   - Что было сделано
   - Какую проблему это решает
   - Как тестировались изменения
   - Ссылки на связанные issues (используйте ключевые слова "Fixes #123" или "Resolves #123")

## Стиль кода

### Rust

- Используйте `cargo fmt` для форматирования кода
- Следуйте рекомендациям [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Используйте `cargo clippy` для проверки потенциальных проблем

### JavaScript/TypeScript/Vue

- Используйте ESLint для проверки кода
- Следуйте рекомендациям [Vue Style Guide](https://vuejs.org/style-guide/)

## Процесс рассмотрения PR

- Каждый PR должен пройти CI проверки
- Необходим как минимум один апрув от мейнтейнера
- При необходимости могут быть запрошены изменения
- После одобрения PR будет объединен мейнтейнером

## Вопросы и обсуждения

Для обсуждения идей и задач используйте GitHub Discussions. 

## Лицензия

Внося свой вклад в проект, вы соглашаетесь с тем, что ваш код будет распространяться под лицензией MIT проекта. 