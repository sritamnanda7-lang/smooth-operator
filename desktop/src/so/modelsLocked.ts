// SPDX-License-Identifier: FSL-1.1-MIT
// Handy locked model switching while a background file transcription ran. That
// feature is stripped, so nothing ever locks the models. Kept as a seam so the
// model screens keep their lock handling until a later step removes it.
export const useModelsLocked = (): boolean => false;
export const areModelsLocked = (): boolean => false;
