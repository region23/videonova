#include <soundtouch/SoundTouch.h>

using namespace soundtouch;

extern "C" {
    void* soundtouch_createInstance() {
        return new SoundTouch();
    }

    void soundtouch_destroyInstance(void* instance) {
        delete static_cast<SoundTouch*>(instance);
    }

    void soundtouch_setSampleRate(void* instance, unsigned int srate) {
        static_cast<SoundTouch*>(instance)->setSampleRate(srate);
    }

    void soundtouch_setChannels(void* instance, unsigned int numChannels) {
        static_cast<SoundTouch*>(instance)->setChannels(numChannels);
    }

    void soundtouch_setTempo(void* instance, float newTempo) {
        static_cast<SoundTouch*>(instance)->setTempo(newTempo);
    }

    void soundtouch_setPitch(void* instance, float newPitch) {
        static_cast<SoundTouch*>(instance)->setPitch(newPitch);
    }

    void soundtouch_putSamples(void* instance, const float* samples, unsigned int numSamples) {
        static_cast<SoundTouch*>(instance)->putSamples(samples, numSamples);
    }

    unsigned int soundtouch_receiveSamples(void* instance, float* outBuffer, unsigned int maxSamples) {
        return static_cast<SoundTouch*>(instance)->receiveSamples(outBuffer, maxSamples);
    }
} 