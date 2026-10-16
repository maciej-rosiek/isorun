import { SoundEngine } from './engine'

/** The one sound engine of the page. */
export const audio = new SoundEngine(`${import.meta.env.BASE_URL}audio/`)
