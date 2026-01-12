#!/usr/bin/env node
// Simple test script that mimics ink's TextInput behavior
// Run with: node tests/ink_input_test.js

const readline = require('readline');

// Put terminal in raw mode like ink does
if (process.stdin.isTTY) {
    process.stdin.setRawMode(true);
}
process.stdin.resume();

let buffer = '';
let submitted = false;

console.log('Waiting for input... (press Enter to submit, Ctrl+C to exit)');
console.log('Raw mode:', process.stdin.isRaw);

process.stdin.on('data', (data) => {
    // Log raw bytes received
    console.log('Received bytes:', Array.from(data).map(b => b.toString(16).padStart(2, '0')).join(' '));
    console.log('Received string:', JSON.stringify(data.toString()));

    for (const byte of data) {
        if (byte === 0x03) {
            // Ctrl+C
            console.log('\nExiting...');
            process.exit(0);
        } else if (byte === 0x0d || byte === 0x0a) {
            // Enter (CR or LF)
            console.log(`\nSUBMITTED: "${buffer}"`);
            submitted = true;
            buffer = '';
        } else if (byte === 0x7f || byte === 0x08) {
            // Backspace
            buffer = buffer.slice(0, -1);
            console.log(`Buffer after backspace: "${buffer}"`);
        } else if (byte >= 0x20 && byte < 0x7f) {
            // Printable ASCII
            buffer += String.fromCharCode(byte);
            console.log(`Buffer: "${buffer}"`);
        } else {
            console.log(`Ignoring byte: 0x${byte.toString(16)}`);
        }
    }
});

// Exit after 30 seconds
setTimeout(() => {
    console.log('\nTimeout - exiting');
    process.exit(0);
}, 30000);
