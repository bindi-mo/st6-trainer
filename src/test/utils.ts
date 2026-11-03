import { vi } from 'vitest'

/**
 * Test utilities for Tauri API mocking
 */

// Mock invoke responses
export const mockInvoke = (responses: Record<string, any>) => {
    const { invoke } = await import('@tauri-apps/api/core')
    vi.mocked(invoke).mockImplementation(async (command: string, args?: any) => {
        if (responses[command] !== undefined) {
            return typeof responses[command] === 'function'
                ? responses[command](args)
                : responses[command]
        }
        throw new Error(`No mock response for command: ${command}`)
    })
    return invoke
}

// Reset all mocks
export const resetMocks = () => {
    vi.clearAllMocks()
}

// Mock Tauri event system
export const mockTauriEvent = () => {
    const listeners = new Map<string, Function[]>()

    const { listen, emit } = await import('@tauri-apps/api/event')

    vi.mocked(listen).mockImplementation(async (event: string, handler: Function) => {
        if (!listeners.has(event)) {
            listeners.set(event, [])
        }
        listeners.get(event)!.push(handler)
        return () => {
            const handlers = listeners.get(event) || []
            const index = handlers.indexOf(handler)
            if (index > -1) handlers.splice(index, 1)
        }
    })

    vi.mocked(emit).mockImplementation(async (event: string, payload?: any) => {
        const handlers = listeners.get(event) || []
        handlers.forEach(h => h(payload))
    })

    return { listeners, listen, emit }
}

// Helper to wait for async operations
export const waitFor = (ms: number = 0) => new Promise(resolve => setTimeout(resolve, ms))

// Mock file system for testing
export const createMockFile = (name: string, content: string, type: string = 'text/plain') => {
    return new File([content], name, { type })
}