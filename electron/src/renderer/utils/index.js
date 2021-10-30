const ppmRate = 500 / 1800

export const ms2ppm = (ms) => Math.floor(ms * ppmRate)
export const ppm2ms = (ppm) => Math.floor(ppm / ppmRate)
