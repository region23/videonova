<template>
  <div v-if="isVisible" class="service-check-container">
    <h3 v-if="!isChecking" class="check-title">Проверка доступности сервисов</h3>
    
    <div v-if="isChecking" class="checking-status">
      <div class="status-indicator">
        <p>Проверка доступности сервисов...</p>
        <div class="loading-spinner"></div>
      </div>
    </div>
    
    <div v-else-if="checkResult" class="check-results">
      <ul class="services-list">
        <li :class="checkResult.youtube_available ? 'available' : 'blocked'">
          <span class="service-name">YouTube:</span> 
          <span class="service-status">{{ checkResult.youtube_available ? 'Доступен' : 'Заблокирован' }}</span>
        </li>
        <li :class="checkResult.openai_available ? 'available' : 'blocked'">
          <span class="service-name">OpenAI:</span> 
          <span class="service-status">{{ checkResult.openai_available ? 'Доступен' : 'Заблокирован' }}</span>
        </li>
      </ul>
      
      <div :class="['message-container', checkResult.vpn_required ? 'warning-message' : 'success-message']">
        <p>{{ checkResult.message }}</p>
        <!-- Показываем кнопку только если требуется VPN или что-то заблокировано -->
        <button 
          v-if="checkResult.vpn_required || !checkResult.youtube_available || !checkResult.openai_available" 
          class="check-button" 
          @click="triggerCheckServices(true)"
        >
          Проверить снова
        </button>
      </div>
    </div>
    
    <div v-else-if="error" class="error-container">
      <div class="error-message">
        <span class="error-icon">⚠️</span>
        <p>{{ error }}</p>
      </div>
      <button class="check-button" @click="triggerCheckServices()">
        Попробовать снова
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue';
import { checkServicesAvailability, setupServiceCheckListeners } from '../utils/serviceAvailability';

// Определяем типы для результатов проверки
interface ServiceAvailabilityResult {
  youtube_available: boolean;
  openai_available: boolean;
  vpn_required: boolean;
  message: string;
  is_retry: boolean;
}

// Определяем события, которые компонент может отправлять
const emit = defineEmits<{
  'hide-service-check': []
}>();

// Реактивные состояния
const isChecking = ref(false);
const checkResult = ref<ServiceAvailabilityResult | null>(null);
const error = ref<string | null>(null);
const isVisible = ref(true);
const initialCheckDone = ref(false); // Флаг для отслеживания первичной проверки

// Ссылка на функцию очистки слушателей событий
let cleanupListeners: (() => Promise<void>) | null = null;
let checkTimeoutId: number | null = null;
let hideTimeoutId: number | null = null;

// Таймаут, после которого запустить проверку самостоятельно, если не получен результат от бэкенда
let autoCheckTimeoutId: number | null = null;

// Добавим наблюдатель за результатом проверки
watch(checkResult, (newResult) => {
  if (newResult) {
    // Если все сервисы доступны и VPN не требуется, скрываем компонент через 3 секунды
    if (newResult.youtube_available && newResult.openai_available && !newResult.vpn_required) {
      clearTimeout(hideTimeoutId as number);
      hideTimeoutId = window.setTimeout(() => {
        isVisible.value = false;
        emit('hide-service-check');
      }, 3000); // Увеличиваем до 3 секунд для лучшей читаемости результата
    }
  }
});

// Функция для проверки доступности сервисов с таймаутом (для кнопки "Проверить снова")
const checkServices = async (isRetry: boolean = false) => {
  // Предотвращаем параллельные проверки
  if (isChecking.value) {
    console.log('Проверка уже выполняется, игнорируем повторный запуск');
    return;
  }
  
  try {
    isChecking.value = true;
    error.value = null;
    console.log(`Ручной запуск проверки сервисов (${isRetry ? 'повторная' : 'первичная'})`);
    
    // Увеличиваем таймаут для проверки до 10 секунд
    const timeoutPromise = new Promise<never>((_, reject) => {
      checkTimeoutId = window.setTimeout(() => {
        reject(new Error("Timeout: проверка сервисов заняла слишком много времени"));
      }, 10000);
    });
    
    // Запускаем проверку с таймаутом
    try {
      const result = await Promise.race([
        checkServicesAvailability(isRetry),
        timeoutPromise
      ]);
      
      // Если успешно получили результат, сохраняем его
      checkResult.value = result as ServiceAvailabilityResult;
      initialCheckDone.value = true;
    } catch (e) {
      // Более дружелюбная обработка таймаута
      if (e instanceof Error && e.message.includes('Timeout')) {
        error.value = "Превышено время ожидания при проверке сервисов. Это может указывать на проблемы с подключением. Попробуйте снова.";
      } else {
        throw e;
      }
    } finally {
      // Очищаем таймаут в любом случае
      if (checkTimeoutId) {
        clearTimeout(checkTimeoutId);
        checkTimeoutId = null;
      }
    }
  } catch (err) {
    console.error('Ошибка при проверке доступности сервисов:', err);
    // Форматируем сообщение ошибки более понятно для пользователя
    if (err instanceof Error) {
      if (err.message.includes('Timeout')) {
        error.value = "Превышено время ожидания при проверке сервисов. Это может указывать на проблемы с подключением. Попробуйте снова.";
      } else {
        error.value = `Ошибка проверки: ${err.message}`;
      }
    } else {
      error.value = `Ошибка проверки: ${String(err)}`;
    }
    checkResult.value = null;
  } finally {
    isChecking.value = false;
    console.log('Проверка сервисов завершена');
  }
};

// Функция, которую вызывают UI-события - обертка над checkServices
const triggerCheckServices = (isRetry: boolean = false) => {
  // Отменяем любой таймаут скрытия, если проверка запущена снова
  if (hideTimeoutId) {
    clearTimeout(hideTimeoutId);
    hideTimeoutId = null;
  }
  
  // Восстанавливаем видимость компонента, если он был скрыт
  isVisible.value = true;
  
  // Запускаем проверку неблокирующим образом
  setTimeout(() => {
    checkServices(isRetry).catch(err => {
      console.error('Ошибка при запуске проверки:', err);
    });
  }, 0);
};

// Настройка слушателей событий и первичная проверка
onMounted(() => {
  console.log('ServiceAvailabilityCheck mounting');
  
  // Настраиваем слушателей событий - делаем это неблокирующим
  setTimeout(async () => {
    try {
      // Устанавливаем слушателей без запуска проверки
      cleanupListeners = await setupServiceCheckListeners({
        onCheckStarted: (isRetry) => {
          console.log(`Начата ${isRetry ? 'повторная' : 'первичная'} проверка сервисов (от слушателя событий)`);
          isChecking.value = true;
          
          // Если проверка запущена, отменяем запланированный таймаут на авто-проверку
          if (autoCheckTimeoutId) {
            clearTimeout(autoCheckTimeoutId);
            autoCheckTimeoutId = null;
          }
        },
        onYouTubeResult: (available) => {
          console.log(`YouTube доступен: ${available}`);
        },
        onOpenAIResult: (available) => {
          console.log(`OpenAI доступен: ${available}`);
        },
        onCheckCompleted: (result) => {
          console.log('Проверка завершена (от слушателя событий):', result);
          isChecking.value = false;
          
          // Make sure the result has all required properties before assigning
          if (typeof result === 'object' && 
              'youtube_available' in result && 
              'openai_available' in result && 
              'vpn_required' in result &&
              'message' in result) {
            // This satisfies the type constraint
            console.log('Получены данные о доступности сервисов:', {
              youtube: result.youtube_available,
              openai: result.openai_available,
              vpnRequired: result.vpn_required,
              message: result.message
            });
            
            // Обновляем результат проверки
            checkResult.value = result as ServiceAvailabilityResult;
            initialCheckDone.value = true;
            
            // Если получен результат, отменяем запланированный таймаут на авто-проверку
            if (autoCheckTimeoutId) {
              clearTimeout(autoCheckTimeoutId);
              autoCheckTimeoutId = null;
            }
          } else {
            // Handle case where result doesn't match expected structure
            console.error('Unexpected result format from service check:', result);
            console.error('Missing properties:', {
              hasYouTube: result && typeof result === 'object' ? 'youtube_available' in result : false,
              hasOpenAI: result && typeof result === 'object' ? 'openai_available' in result : false,
              hasVpnRequired: result && typeof result === 'object' ? 'vpn_required' in result : false,
              hasMessage: result && typeof result === 'object' ? 'message' in result : false,
              rawResult: JSON.stringify(result)
            });
            error.value = 'Получен неверный формат данных при проверке сервисов';
          }
        }
      });
      
      // Установка таймаута для самостоятельного запуска проверки, если она не была запущена из main.rs
      autoCheckTimeoutId = window.setTimeout(() => {
        if (!initialCheckDone.value && !isChecking.value) {
          console.log('Автоматическая проверка не запустилась в течение 5 секунд, запускаем вручную');
          triggerCheckServices(false);
        }
      }, 5000); // 5 секунд ожидания, после чего запускаем проверку вручную
      
      console.log('Ожидаем результаты от автоматической проверки...');
    } catch (err) {
      console.error('Ошибка при установке слушателей:', err);
      error.value = `Ошибка настройки: ${err instanceof Error ? err.message : String(err)}`;
    }
  }, 100); // Небольшая задержка для установки слушателей
});

// Очистка слушателей при размонтировании компонента
onUnmounted(() => {
  console.log('ServiceAvailabilityCheck unmounting');
  
  // Очищаем таймаут проверки, если он установлен
  if (checkTimeoutId) {
    clearTimeout(checkTimeoutId);
    checkTimeoutId = null;
  }
  
  // Очищаем таймаут скрытия, если он установлен
  if (hideTimeoutId) {
    clearTimeout(hideTimeoutId);
    hideTimeoutId = null;
  }
  
  // Очищаем таймаут авто-проверки, если он установлен
  if (autoCheckTimeoutId) {
    clearTimeout(autoCheckTimeoutId);
    autoCheckTimeoutId = null;
  }
  
  // Очищаем слушателей неблокирующим образом
  if (cleanupListeners) {
    setTimeout(async () => {
      try {
        await cleanupListeners?.();
      } catch (err) {
        console.error('Ошибка при очистке слушателей:', err);
      }
    }, 0);
  }
});
</script>

<style scoped>
.service-check-container {
  background-color: var(--background-secondary, #f5f5f5);
  border-radius: 12px;
  padding: 0.75rem;
  margin: auto 0;
  border: 1px solid rgba(0, 0, 0, 0.05);
  font-size: 0.85rem;
  position: relative;
  min-height: 120px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

/* Добавим дополнительный класс-обертку для родителя, если он используется */
:deep(.service-check-wrapper) {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}

.check-title {
  font-size: 0.85rem;
  font-weight: normal;
  color: var(--text-primary);
  margin: 0 0 0.5rem;
  letter-spacing: -0.01em;
  text-align: center;
}

.checking-status {
  display: flex;
  justify-content: center;
  align-items: center;
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  height: 100%;
}

.status-indicator {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1rem;
  text-align: center;
}

.loading-spinner {
  width: 24px;
  height: 24px;
  border: 2px solid rgba(0, 119, 255, 0.3);
  border-top-color: var(--primary-color, #0077ff);
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.check-results {
  display: flex;
  flex-direction: column;
  align-items: center;
}

.services-list {
  list-style: none;
  padding: 0;
  margin: 0 0 0.75rem;
  width: 100%;
}

.services-list li {
  padding: 0.5rem;
  margin-bottom: 0.5rem;
  border-radius: 4px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

li.available {
  background-color: rgba(76, 217, 100, 0.1);
  color: var(--success-color, #4cd964);
  border: 1px solid var(--success-color, #4cd964);
}

li.blocked {
  background-color: rgba(255, 59, 48, 0.1);
  color: var(--error-color, #ff3b30);
  border: 1px solid var(--error-color, #ff3b30);
}

.service-name {
  font-weight: 600;
}

.message-container {
  padding: 0.5rem;
  border-radius: 6px;
  margin-bottom: 0.5rem;
  width: 100%;
  text-align: center;
}

.warning-message {
  background-color: rgba(255, 204, 0, 0.1);
  border: 1px solid var(--warning-color, #ffcc00);
}

.success-message {
  background-color: rgba(76, 217, 100, 0.1);
  border: 1px solid var(--success-color, #4cd964);
}

.error-container {
  padding: 0.5rem;
  margin-bottom: 0.5rem;
  border-radius: 6px;
  background-color: rgba(255, 59, 48, 0.1);
  border: 1px solid var(--error-color, #ff3b30);
  text-align: center;
  width: 100%;
}

.error-message {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.error-icon {
  font-size: 1.25rem;
  flex-shrink: 0;
}

.error-message p {
  margin: 0;
  text-align: center;
}

.check-button {
  background-color: var(--primary-color, #0077ff);
  color: white;
  border: none;
  border-radius: 4px;
  padding: 0.4rem 0.75rem;
  font-size: 0.8rem;
  cursor: pointer;
  transition: background-color 0.2s;
  display: block;
  margin: 0.5rem auto 0;
}

.check-button:hover {
  background-color: var(--primary-color-dark, #0066dd);
}
</style> 