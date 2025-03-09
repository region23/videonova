document.addEventListener('DOMContentLoaded', function() {
  // Плавная прокрутка для якорных ссылок
  const smoothScrollLinks = document.querySelectorAll('a[href^="#"]');
  
  smoothScrollLinks.forEach(link => {
    link.addEventListener('click', function(e) {
      e.preventDefault();
      
      const targetId = this.getAttribute('href');
      if (targetId === '#') return;
      
      const targetElement = document.querySelector(targetId);
      if (targetElement) {
        window.scrollTo({
          top: targetElement.offsetTop - 80,
          behavior: 'smooth'
        });
      }
    });
  });
  
  // Header становится непрозрачным при прокрутке
  const header = document.querySelector('header');
  
  window.addEventListener('scroll', function() {
    if (window.scrollY > 10) {
      header.classList.add('scrolled');
    } else {
      header.classList.remove('scrolled');
    }
  });
  
  // Анимация появления элементов при прокрутке
  const animatedElements = document.querySelectorAll('.animate-on-scroll');
  
  function checkIfInView() {
    animatedElements.forEach(element => {
      const elementTop = element.getBoundingClientRect().top;
      const elementVisible = 150;
      
      if (elementTop < window.innerHeight - elementVisible) {
        element.classList.add('visible');
      }
    });
  }
  
  window.addEventListener('scroll', checkIfInView);
  checkIfInView();
  
  // Галерея скриншотов
  const screenshotsGallery = document.querySelector('.screenshots-gallery');
  if (screenshotsGallery) {
    const galleryImages = screenshotsGallery.querySelectorAll('.hero-image');
    const galleryDots = screenshotsGallery.querySelectorAll('.gallery-dot');
    let currentIndex = 0;
    let interval;
    
    function showImage(index) {
      // Скрываем все изображения и точки
      galleryImages.forEach(img => img.classList.remove('active'));
      galleryDots.forEach(dot => dot.classList.remove('active'));
      
      // Показываем выбранное изображение и точку
      galleryImages[index].classList.add('active');
      galleryDots[index].classList.add('active');
      
      currentIndex = index;
    }
    
    function nextImage() {
      let nextIndex = currentIndex + 1;
      if (nextIndex >= galleryImages.length) {
        nextIndex = 0;
      }
      showImage(nextIndex);
    }
    
    // Обработка клика по точкам
    galleryDots.forEach((dot, index) => {
      dot.addEventListener('click', () => {
        showImage(index);
        clearInterval(interval);
        startAutoSlide();
      });
    });
    
    // Автоматическое переключение слайдов
    function startAutoSlide() {
      interval = setInterval(nextImage, 5000); // Переключаем каждые 5 секунд
    }
    
    // Запускаем автоматическое переключение
    startAutoSlide();
    
    // Остановка автоматического переключения при наведении
    screenshotsGallery.addEventListener('mouseenter', () => {
      clearInterval(interval);
    });
    
    // Возобновление автоматического переключения при уходе курсора
    screenshotsGallery.addEventListener('mouseleave', () => {
      clearInterval(interval);
      startAutoSlide();
    });
  }
}); 