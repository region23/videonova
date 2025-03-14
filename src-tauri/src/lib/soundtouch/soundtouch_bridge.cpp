#include <SoundTouch.h>
#include <cstdint>

using namespace soundtouch;

extern "C" {
    SoundTouch* soundtouch_createInstance() {
        return new SoundTouch();
    }

    void soundtouch_destroyInstance(SoundTouch* instance) {
        delete instance;
    }

    void soundtouch_setSampleRate(SoundTouch* instance, uint32_t srate) {
        instance->setSampleRate(srate);
    }

    void soundtouch_setChannels(SoundTouch* instance, uint32_t numChannels) {
        instance->setChannels(numChannels);
    }

    void soundtouch_setTempo(SoundTouch* instance, float newTempo) {
        instance->setTempo(newTempo);
    }

    void soundtouch_setPitch(SoundTouch* instance, float newPitch) {
        instance->setPitch(newPitch);
    }

    void soundtouch_putSamples(SoundTouch* instance, const float* samples, uint32_t numSamples) {
        instance->putSamples(samples, numSamples);
    }

    uint32_t soundtouch_receiveSamples(SoundTouch* instance, float* outBuffer, uint32_t maxSamples) {
        return instance->receiveSamples(outBuffer, maxSamples);
    }
} 