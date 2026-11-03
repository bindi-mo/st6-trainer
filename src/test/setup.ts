import '@testing-library/jest-dom'
import { vi } from 'vitest'

// Mock Tauri APIs for testing
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn(),
    emit: vi.fn(),
}))

// Mock window.__TAURI__ for tests
Object.defineProperty(window, '__TAURI__', {
    value: {
        invoke: vi.fn(),
        event: {
            listen: vi.fn(),
            emit: vi.fn(),
        },
    },
    writable: true,
})