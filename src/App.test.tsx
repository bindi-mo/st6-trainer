import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import App from "./App";

describe('App', () => {
    it('renders without crashing', () => {
        render(<App />)
        expect(screen.getByText('Welcome to Tauri + React')).toBeInTheDocument()
    })

    it('has greet input and button', () => {
        render(<App />)
        expect(screen.getByPlaceholderText('Enter a name...')).toBeInTheDocument()
        expect(screen.getByRole('button', { name: /greet/i })).toBeInTheDocument()
    })

    it('calls invoke when greet is submitted', async () => {
        const { invoke } = await import('@tauri-apps/api/core')
        vi.mocked(invoke).mockResolvedValue('Hello, Test!')

        render(<App />)
        const input = screen.getByPlaceholderText('Enter a name...')
        const button = screen.getByRole('button', { name: /greet/i })

        fireEvent.change(input, { target: { value: 'Test' } })
        fireEvent.click(button)

        expect(invoke).toHaveBeenCalledWith('greet', { name: 'Test' })
    })
})