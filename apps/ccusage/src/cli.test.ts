import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { createRequire } from 'node:module';
import { describe, it, mock } from 'node:test';
import { pathToFileURL } from 'node:url';
import {
	createNativeSpawner,
	ensureNativeBinaryExecutable,
	isMainModule,
	resolveCliRuntime,
	resolveNativeBinary,
} from './cli.js';

const require = createRequire(import.meta.url);
/** @type {{ version: string }} */
const packageJson = require('../package.json');

void describe(resolveCliRuntime.name, () => {
	void it('resolves the published Windows x64 native package binary', () => {
		const actual = resolveNativeBinary({
			arch: 'x64',
			platform: 'win32',
			resolvePath: (id) => {
				assert.equal(id, 'ccusage-mostafa-win32-x64/bin/ccusage.exe');
				return 'C:\\native\\bin\\ccusage.exe';
			},
		});

		assert.equal(actual, 'C:\\native\\bin\\ccusage.exe');
	});

	void it('prefers the matching native package binary when it is available', () => {
		assert.deepEqual(
			resolveCliRuntime({
				argv: ['daily'],
				nativeBinaryPath: '/app/node_modules/ccusage-mostafa-win32-x64/bin/ccusage.exe',
			}),
			{
				args: ['daily'],
				command: '/app/node_modules/ccusage-mostafa-win32-x64/bin/ccusage.exe',
			},
		);
	});

	void it('fails when the native package binary is unavailable', () => {
		assert.deepEqual(
			resolveCliRuntime({
				arch: 'x64',
				argv: ['daily'],
				nativeBinaryPath: null,
				platform: 'win32',
			}),
			{
				errorMessage: `Missing optional native package "ccusage-mostafa-win32-x64@${packageJson.version}" for win32-x64. It may be absent because of npm --before/--min-release-age, mise minimum_release_age, or --omit=optional. Reinstall ccusage-mostafa with optional dependencies enabled and no release-age cutoff.\n`,
			},
		);
	});

	void it('reports unsupported native binary targets without suggesting an optional package', () => {
		assert.deepEqual(
			resolveCliRuntime({
				arch: 'riscv64',
				argv: [],
				nativeBinaryPath: null,
				platform: 'freebsd',
			}),
			{
				errorMessage: 'ccusage-mostafa does not provide a native binary for freebsd-riscv64.\n',
			},
		);
	});

	void it('repairs a native binary that was extracted without executable bits', () => {
		const chmodPath = mock.fn();

		assert.equal(
			ensureNativeBinaryExecutable({
				binaryPath: '/native/bin/ccusage',
				chmodPath,
				platform: 'linux',
				statPath: () => ({ mode: 0o644 }),
			}),
			undefined,
		);
		assert.deepEqual(
			chmodPath.mock.calls.map((call) => call.arguments),
			[['/native/bin/ccusage', 0o755]],
		);
	});

	void it('does not chmod an already executable native binary', () => {
		const chmodPath = mock.fn();

		assert.equal(
			ensureNativeBinaryExecutable({
				binaryPath: '/native/bin/ccusage',
				chmodPath,
				platform: 'darwin',
				statPath: () => ({ mode: 0o755 }),
			}),
			undefined,
		);
		assert.equal(chmodPath.mock.callCount(), 0);
	});

	void it('does not chmod Windows native binaries', () => {
		const chmodPath = mock.fn();

		assert.equal(
			ensureNativeBinaryExecutable({
				binaryPath: 'C:\\native\\bin\\ccusage.exe',
				chmodPath,
				platform: 'win32',
				statPath: () => ({ mode: 0o644 }),
			}),
			undefined,
		);
		assert.equal(chmodPath.mock.callCount(), 0);
	});

	void it('treats package bin symlinks as the main module entry point', () => {
		const modulePath = process.platform === 'win32'
			? 'C:\\project\\node_modules\\ccusage-mostafa\\src\\cli.js'
			: '/project/node_modules/ccusage-mostafa/src/cli.js';
		const binPath = process.platform === 'win32'
			? 'C:\\project\\node_modules\\.bin\\ccusage-mostafa'
			: '/project/node_modules/.bin/ccusage-mostafa';
		const actual = isMainModule({
			argvEntry: binPath,
			moduleUrl: pathToFileURL(modulePath).href,
			realpathPath: (path) =>
				path === binPath
					? modulePath
					: path,
		});

		assert.equal(actual, true);
	});
});

void describe(createNativeSpawner.name, () => {
	void it('forwards every supported launcher signal delivery', async () => {
		const signalSource = new EventEmitter();
		const kill = mock.fn(() => true);
		const child = /** @type {import('node:child_process').ChildProcess} */ (
			/** @type {unknown} */ (Object.assign(new EventEmitter(), { kill }))
		);
		const spawnProcess = mock.fn(() => child);
		const spawnNative = createNativeSpawner({
			platform: 'linux',
			signalSource,
			spawnProcess,
		});

		const resultPromise = spawnNative('/native/bin/ccusage', ['statusline']);
		for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP', 'SIGQUIT']) {
			signalSource.emit(signal);
			signalSource.emit(signal);
		}
		child.emit('exit', 0, null);

		assert.deepEqual(await resultPromise, { signal: null, status: 0 });
		assert.deepEqual(
			kill.mock.calls.map((call) => call.arguments),
			[
				['SIGINT'],
				['SIGINT'],
				['SIGTERM'],
				['SIGTERM'],
				['SIGHUP'],
				['SIGHUP'],
				['SIGQUIT'],
				['SIGQUIT'],
			],
		);
		const spawnCall = spawnProcess.mock.calls[0];
		assert.ok(spawnCall);
		assert.deepEqual(spawnCall.arguments, [
			'/native/bin/ccusage',
			['statusline'],
			{ stdio: 'inherit' },
		]);
	});

	void it('forwards the supported Windows console signals', async () => {
		const signalSource = new EventEmitter();
		const kill = mock.fn(() => true);
		const child = /** @type {import('node:child_process').ChildProcess} */ (
			/** @type {unknown} */ (Object.assign(new EventEmitter(), { kill }))
		);
		const spawnNative = createNativeSpawner({
			platform: 'win32',
			signalSource,
			spawnProcess: () => child,
		});

		const resultPromise = spawnNative('/native/bin/ccusage.exe', []);
		assert.deepEqual(
			['SIGINT', 'SIGBREAK', 'SIGHUP'].map((signal) => signalSource.listenerCount(signal)),
			[1, 1, 1],
		);
		signalSource.emit('SIGINT');
		signalSource.emit('SIGBREAK');
		signalSource.emit('SIGHUP');
		signalSource.emit('SIGTERM');
		assert.equal(signalSource.listenerCount('SIGTERM'), 0);
		assert.deepEqual(
			kill.mock.calls.map((call) => call.arguments),
			[['SIGINT'], ['SIGBREAK'], ['SIGHUP']],
		);
		child.emit('exit', 0, null);

		assert.deepEqual(await resultPromise, { signal: null, status: 0 });
		assert.deepEqual(
			['SIGINT', 'SIGBREAK', 'SIGHUP'].map((signal) => signalSource.listenerCount(signal)),
			[0, 0, 0],
		);
	});

	void it('removes signal listeners after the child exits', async () => {
		const signalSource = new EventEmitter();
		const kill = mock.fn(() => true);
		const child = /** @type {import('node:child_process').ChildProcess} */ (
			/** @type {unknown} */ (Object.assign(new EventEmitter(), { kill }))
		);
		const spawnNative = createNativeSpawner({
			platform: 'linux',
			signalSource,
			spawnProcess: () => child,
		});

		const resultPromise = spawnNative('/native/bin/ccusage', []);
		child.emit('exit', 7, 'SIGTERM');

		assert.deepEqual(await resultPromise, { signal: 'SIGTERM', status: 7 });
		assert.deepEqual(
			['SIGINT', 'SIGTERM', 'SIGHUP', 'SIGQUIT'].map((signal) =>
				signalSource.listenerCount(signal),
			),
			[0, 0, 0, 0],
		);
		signalSource.emit('SIGTERM');
		assert.equal(kill.mock.callCount(), 0);
	});

	void it('removes signal listeners after a child error', async () => {
		const signalSource = new EventEmitter();
		const kill = mock.fn(() => true);
		const child = /** @type {import('node:child_process').ChildProcess} */ (
			/** @type {unknown} */ (Object.assign(new EventEmitter(), { kill }))
		);
		const spawnNative = createNativeSpawner({
			platform: 'linux',
			signalSource,
			spawnProcess: () => child,
		});
		const error = new Error('spawn failed');

		const resultPromise = spawnNative('/native/bin/ccusage', []);
		child.emit('error', error);

		assert.deepEqual(await resultPromise, {
			error,
			signal: null,
			status: null,
		});
		assert.deepEqual(
			['SIGINT', 'SIGTERM', 'SIGHUP', 'SIGQUIT'].map((signal) =>
				signalSource.listenerCount(signal),
			),
			[0, 0, 0, 0],
		);
		signalSource.emit('SIGINT');
		assert.equal(kill.mock.callCount(), 0);
	});
});
