import { create } from 'zustand'

type Theme = 'light' | 'dark'
type ConsoleState = {
  theme: Theme
  token: string
  setTheme: (theme: Theme) => void
  setToken: (token: string) => void
}

const initialTheme = (localStorage.getItem('business-console-theme') as Theme | null) ?? 'light'

export const useConsoleStore = create<ConsoleState>((set) => ({
  theme: initialTheme,
  token: import.meta.env.VITE_BUSINESS_API_TOKEN ?? 'dev-only-secret',
  setTheme: (theme) => {
    localStorage.setItem('business-console-theme', theme)
    set({ theme })
  },
  setToken: (token) => set({ token }),
}))
