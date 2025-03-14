import { invoke } from '@tauri-apps/api/core';
import { message } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// Типы для результатов проверки
interface ServiceAvailabilityResult {
  youtube_available: boolean;
  openai_available: boolean;
  vpn_required: boolean;
  message: string;
  is_retry: boolean;
}

// Настройки диалогового окна
interface DialogOptions {
  title?: string;
  kind?: 'info' | 'warning' | 'error';
}

/**
 * Проверяет доступность необходимых сервисов (YouTube и OpenAI)
 * без показа диалоговых окон - за отображение отвечает компонент UI
 * 
 * @param isRetry Указывает, что это повторная проверка после включения VPN
 * @returns Результат проверки доступности сервисов
 */
export async function checkServicesAvailability(isRetry: boolean = false): Promise<ServiceAvailabilityResult> {
  try {
    // Вызываем Rust-функцию для проверки доступности сервисов
    const result = await invoke<ServiceAvailabilityResult>('check_services_availability', { 
      isRetry
    });
    
    // Просто возвращаем результат без показа диалоговых окон
    // UI компонент ServiceAvailabilityCheck сам отобразит необходимую информацию
    return result;
  } catch (error) {
    console.error('Ошибка при проверке доступности сервисов:', error);
    
    // Возвращаем объект с ошибкой без показа диалога
    return {
      youtube_available: false,
      openai_available: false,
      vpn_required: true,
      message: `Ошибка проверки: ${error}`,
      is_retry: isRetry
    };
  }
}

/**
 * Настраивает слушателей событий для отслеживания процесса проверки
 * Можно использовать для обновления UI во время проверки
 * 
 * @param callbacks Объект с функциями обратного вызова для различных событий
 * @returns Функция для удаления слушателей событий
 */
export function setupServiceCheckListeners(callbacks: {
  onCheckStarted?: (isRetry: boolean) => void;
  onYouTubeChecking?: () => void;
  onYouTubeResult?: (available: boolean) => void;
  onOpenAIChecking?: () => void;
  onOpenAIResult?: (available: boolean) => void;
  onCheckCompleted?: (result: ServiceAvailabilityResult) => void;
}): Promise<() => Promise<void>> {
  // Массив функций для удаления слушателей
  const unlisteners: Promise<UnlistenFn>[] = [];
  
  // Слушаем событие запуска проверки из main.rs
  unlisteners.push(listen('check-services-availability', (event) => {
    const data = event.payload as { isRetry: boolean };
    // Вызываем команду check_services_availability, когда получаем событие
    // Это позволяет запустить проверку, когда main.rs отправляет событие
    invoke<ServiceAvailabilityResult>('check_services_availability', { 
      isRetry: data.isRetry || false 
    }).catch(error => {
      console.error('Ошибка при запуске проверки из события check-services-availability:', error);
    });
  }));
  
  // Слушаем событие начала проверки
  unlisteners.push(listen('services-check-started', (event) => {
    const data = event.payload as { is_retry: boolean };
    callbacks.onCheckStarted?.(data.is_retry);
  }));
  
  // Слушаем события проверки YouTube
  unlisteners.push(listen('checking-youtube', () => {
    callbacks.onYouTubeChecking?.();
  }));
  
  unlisteners.push(listen('youtube-check-complete', (event) => {
    callbacks.onYouTubeResult?.(event.payload as boolean);
  }));
  
  // Слушаем события проверки OpenAI
  unlisteners.push(listen('checking-openai', () => {
    callbacks.onOpenAIChecking?.();
  }));
  
  unlisteners.push(listen('openai-check-complete', (event) => {
    callbacks.onOpenAIResult?.(event.payload as boolean);
  }));
  
  // Слушаем событие завершения проверки
  unlisteners.push(listen('services-check-completed', (event) => {
    // Обновляем типизацию, чтобы включить все поля, которые отправляет бэкенд
    const data = event.payload as ServiceAvailabilityResult;
    callbacks.onCheckCompleted?.(data);
  }));
  
  // Возвращаем функцию для удаления всех слушателей
  return Promise.all(unlisteners).then((unlisten) => {
    return async () => {
      for (const fn of unlisten) {
        await fn();
      }
    };
  });
}

/**
 * Пример использования в компоненте Vue 3:
 * 
 * <script setup lang="ts">
 * import { ref, onMounted, onUnmounted } from 'vue';
 * import { checkServicesAvailability, setupServiceCheckListeners } from '../utils/serviceAvailability';
 * 
 * const isChecking = ref(false);
 * const checkResult = ref(null);
 * 
 * let cleanupListeners: (() => Promise<void>) | null = null;
 * 
 * onMounted(async () => {
 *   // Настраиваем слушателей событий
 *   cleanupListeners = await setupServiceCheckListeners({
 *     onCheckStarted: (isRetry) => {
 *       console.log(`Начата ${isRetry ? 'повторная' : 'первичная'} проверка сервисов`);
 *       isChecking.value = true;
 *     },
 *     onCheckCompleted: (result) => {
 *       console.log('Проверка завершена:', result);
 *       isChecking.value = false;
 *     }
 *   });
 *   
 *   // Выполняем проверку при запуске компонента
 *   checkResult.value = await checkServicesAvailability();
 * });
 * 
 * onUnmounted(async () => {
 *   // Удаляем слушателей событий при уничтожении компонента
 *   if (cleanupListeners) {
 *     await cleanupListeners();
 *   }
 * });
 * 
 * const checkServicesAgain = async (isRetry = false) => {
 *   checkResult.value = await checkServicesAvailability(isRetry);
 * };
 * </script>
 */ 