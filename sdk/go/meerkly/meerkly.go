package meerkly

// #include <meerkly.h>
import "C"

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"math"
	"runtime"
	"runtime/cgo"
	"sync/atomic"
	"unsafe"
)

// This is needed, because as of go 1.24
// type RustBuffer C.RustBuffer cannot have methods,
// RustBuffer is treated as non-local type
type GoRustBuffer struct {
	inner C.RustBuffer
}

type RustBufferI interface {
	AsReader() *bytes.Reader
	Free()
	ToGoBytes() []byte
	Data() unsafe.Pointer
	Len() uint64
	Capacity() uint64
}

func RustBufferFromExternal(b RustBufferI) GoRustBuffer {
	return GoRustBuffer{
		inner: C.RustBuffer{
			capacity: C.uint64_t(b.Capacity()),
			len:      C.uint64_t(b.Len()),
			data:     (*C.uchar)(b.Data()),
		},
	}
}

func (cb GoRustBuffer) Capacity() uint64 {
	return uint64(cb.inner.capacity)
}

func (cb GoRustBuffer) Len() uint64 {
	return uint64(cb.inner.len)
}

func (cb GoRustBuffer) Data() unsafe.Pointer {
	return unsafe.Pointer(cb.inner.data)
}

func (cb GoRustBuffer) AsReader() *bytes.Reader {
	b := unsafe.Slice((*byte)(cb.inner.data), C.uint64_t(cb.inner.len))
	return bytes.NewReader(b)
}

func (cb GoRustBuffer) Free() {
	rustCall(func(status *C.RustCallStatus) bool {
		C.ffi_meerkly_rustbuffer_free(cb.inner, status)
		return false
	})
}

func (cb GoRustBuffer) ToGoBytes() []byte {
	return C.GoBytes(unsafe.Pointer(cb.inner.data), C.int(cb.inner.len))
}

func stringToRustBuffer(str string) C.RustBuffer {
	return bytesToRustBuffer([]byte(str))
}

func bytesToRustBuffer(b []byte) C.RustBuffer {
	if len(b) == 0 {
		return C.RustBuffer{}
	}
	// We can pass the pointer along here, as it is pinned
	// for the duration of this call
	foreign := C.ForeignBytes{
		len:  C.int(len(b)),
		data: (*C.uchar)(unsafe.Pointer(&b[0])),
	}

	return rustCall(func(status *C.RustCallStatus) C.RustBuffer {
		return C.ffi_meerkly_rustbuffer_from_bytes(foreign, status)
	})
}

type BufLifter[GoType any] interface {
	Lift(value RustBufferI) GoType
}

type BufLowerer[GoType any] interface {
	Lower(value GoType) C.RustBuffer
}

type BufReader[GoType any] interface {
	Read(reader io.Reader) GoType
}

type BufWriter[GoType any] interface {
	Write(writer io.Writer, value GoType)
}

func LowerIntoRustBuffer[GoType any](bufWriter BufWriter[GoType], value GoType) C.RustBuffer {
	// This might be not the most efficient way but it does not require knowing allocation size
	// beforehand
	var buffer bytes.Buffer
	bufWriter.Write(&buffer, value)

	bytes, err := io.ReadAll(&buffer)
	if err != nil {
		panic(fmt.Errorf("reading written data: %w", err))
	}
	return bytesToRustBuffer(bytes)
}

func LiftFromRustBuffer[GoType any](bufReader BufReader[GoType], rbuf RustBufferI) GoType {
	defer rbuf.Free()
	reader := rbuf.AsReader()
	item := bufReader.Read(reader)
	if reader.Len() > 0 {
		// TODO: Remove this
		leftover, _ := io.ReadAll(reader)
		panic(fmt.Errorf("Junk remaining in buffer after lifting: %s", string(leftover)))
	}
	return item
}

func rustCallWithError[E any, U any](converter BufReader[*E], callback func(*C.RustCallStatus) U) (U, *E) {
	var status C.RustCallStatus
	returnValue := callback(&status)
	err := checkCallStatus(converter, status)
	return returnValue, err
}

func checkCallStatus[E any](converter BufReader[*E], status C.RustCallStatus) *E {
	switch status.code {
	case 0:
		return nil
	case 1:
		return LiftFromRustBuffer(converter, GoRustBuffer{inner: status.errorBuf})
	case 2:
		// when the rust code sees a panic, it tries to construct a rustBuffer
		// with the message.  but if that code panics, then it just sends back
		// an empty buffer.
		if status.errorBuf.len > 0 {
			panic(fmt.Errorf("%s", FfiConverterStringINSTANCE.Lift(GoRustBuffer{inner: status.errorBuf})))
		} else {
			panic(fmt.Errorf("Rust panicked while handling Rust panic"))
		}
	default:
		panic(fmt.Errorf("unknown status code: %d", status.code))
	}
}

func checkCallStatusUnknown(status C.RustCallStatus) error {
	switch status.code {
	case 0:
		return nil
	case 1:
		panic(fmt.Errorf("function not returning an error returned an error"))
	case 2:
		// when the rust code sees a panic, it tries to construct a C.RustBuffer
		// with the message.  but if that code panics, then it just sends back
		// an empty buffer.
		if status.errorBuf.len > 0 {
			panic(fmt.Errorf("%s", FfiConverterStringINSTANCE.Lift(GoRustBuffer{
				inner: status.errorBuf,
			})))
		} else {
			panic(fmt.Errorf("Rust panicked while handling Rust panic"))
		}
	default:
		return fmt.Errorf("unknown status code: %d", status.code)
	}
}

func rustCall[U any](callback func(*C.RustCallStatus) U) U {
	returnValue, err := rustCallWithError[error](nil, callback)
	if err != nil {
		panic(err)
	}
	return returnValue
}

type NativeError interface {
	AsError() error
}

func writeInt8(writer io.Writer, value int8) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint8(writer io.Writer, value uint8) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeInt16(writer io.Writer, value int16) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint16(writer io.Writer, value uint16) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeInt32(writer io.Writer, value int32) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint32(writer io.Writer, value uint32) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeInt64(writer io.Writer, value int64) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeUint64(writer io.Writer, value uint64) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeFloat32(writer io.Writer, value float32) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func writeFloat64(writer io.Writer, value float64) {
	if err := binary.Write(writer, binary.BigEndian, value); err != nil {
		panic(err)
	}
}

func readInt8(reader io.Reader) int8 {
	var result int8
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint8(reader io.Reader) uint8 {
	var result uint8
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readInt16(reader io.Reader) int16 {
	var result int16
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint16(reader io.Reader) uint16 {
	var result uint16
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readInt32(reader io.Reader) int32 {
	var result int32
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint32(reader io.Reader) uint32 {
	var result uint32
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readInt64(reader io.Reader) int64 {
	var result int64
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readUint64(reader io.Reader) uint64 {
	var result uint64
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readFloat32(reader io.Reader) float32 {
	var result float32
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func readFloat64(reader io.Reader) float64 {
	var result float64
	if err := binary.Read(reader, binary.BigEndian, &result); err != nil {
		panic(err)
	}
	return result
}

func init() {

	uniffiCheckChecksums()
}

func uniffiCheckChecksums() {
	// Get the bindings contract version from our ComponentInterface
	bindingsContractVersion := 26
	// Get the scaffolding contract version by calling the into the dylib
	scaffoldingContractVersion := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint32_t {
		return C.ffi_meerkly_uniffi_contract_version()
	})
	if bindingsContractVersion != int(scaffoldingContractVersion) {
		// If this happens try cleaning and rebuilding your project
		panic("meerkly: UniFFI contract version mismatch")
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_client_key()
		})
		if checksum != 43031 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_client_key: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_connected()
		})
		if checksum != 17032 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_connected: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_gateway_id()
		})
		if checksum != 12787 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_gateway_id: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_start()
		})
		if checksum != 7497 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_start: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_start_blocking()
		})
		if checksum != 15227 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_start_blocking: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_state()
		})
		if checksum != 22483 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_state: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_stop()
		})
		if checksum != 34296 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_stop: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_method_proxyclient_stop_blocking()
		})
		if checksum != 58714 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_method_proxyclient_stop_blocking: UniFFI API checksum mismatch")
		}
	}
	{
		checksum := rustCall(func(_uniffiStatus *C.RustCallStatus) C.uint16_t {
			return C.uniffi_meerkly_checksum_constructor_proxyclient_new()
		})
		if checksum != 23512 {
			// If this happens try cleaning and rebuilding your project
			panic("meerkly: uniffi_meerkly_checksum_constructor_proxyclient_new: UniFFI API checksum mismatch")
		}
	}
}

type FfiConverterUint32 struct{}

var FfiConverterUint32INSTANCE = FfiConverterUint32{}

func (FfiConverterUint32) Lower(value uint32) C.uint32_t {
	return C.uint32_t(value)
}

func (FfiConverterUint32) Write(writer io.Writer, value uint32) {
	writeUint32(writer, value)
}

func (FfiConverterUint32) Lift(value C.uint32_t) uint32 {
	return uint32(value)
}

func (FfiConverterUint32) Read(reader io.Reader) uint32 {
	return readUint32(reader)
}

type FfiDestroyerUint32 struct{}

func (FfiDestroyerUint32) Destroy(_ uint32) {}

type FfiConverterBool struct{}

var FfiConverterBoolINSTANCE = FfiConverterBool{}

func (FfiConverterBool) Lower(value bool) C.int8_t {
	if value {
		return C.int8_t(1)
	}
	return C.int8_t(0)
}

func (FfiConverterBool) Write(writer io.Writer, value bool) {
	if value {
		writeInt8(writer, 1)
	} else {
		writeInt8(writer, 0)
	}
}

func (FfiConverterBool) Lift(value C.int8_t) bool {
	return value != 0
}

func (FfiConverterBool) Read(reader io.Reader) bool {
	return readInt8(reader) != 0
}

type FfiDestroyerBool struct{}

func (FfiDestroyerBool) Destroy(_ bool) {}

type FfiConverterString struct{}

var FfiConverterStringINSTANCE = FfiConverterString{}

func (FfiConverterString) Lift(rb RustBufferI) string {
	defer rb.Free()
	reader := rb.AsReader()
	b, err := io.ReadAll(reader)
	if err != nil {
		panic(fmt.Errorf("reading reader: %w", err))
	}
	return string(b)
}

func (FfiConverterString) Read(reader io.Reader) string {
	length := readInt32(reader)
	buffer := make([]byte, length)
	read_length, err := reader.Read(buffer)
	if err != nil && err != io.EOF {
		panic(err)
	}
	if read_length != int(length) {
		panic(fmt.Errorf("bad read length when reading string, expected %d, read %d", length, read_length))
	}
	return string(buffer)
}

func (FfiConverterString) Lower(value string) C.RustBuffer {
	return stringToRustBuffer(value)
}

func (FfiConverterString) Write(writer io.Writer, value string) {
	if len(value) > math.MaxInt32 {
		panic("String is too large to fit into Int32")
	}

	writeInt32(writer, int32(len(value)))
	write_length, err := io.WriteString(writer, value)
	if err != nil {
		panic(err)
	}
	if write_length != len(value) {
		panic(fmt.Errorf("bad write length when writing string, expected %d, written %d", len(value), write_length))
	}
}

type FfiDestroyerString struct{}

func (FfiDestroyerString) Destroy(_ string) {}

type FfiConverterBytes struct{}

var FfiConverterBytesINSTANCE = FfiConverterBytes{}

func (c FfiConverterBytes) Lower(value []byte) C.RustBuffer {
	return LowerIntoRustBuffer[[]byte](c, value)
}

func (c FfiConverterBytes) Write(writer io.Writer, value []byte) {
	if len(value) > math.MaxInt32 {
		panic("[]byte is too large to fit into Int32")
	}

	writeInt32(writer, int32(len(value)))
	write_length, err := writer.Write(value)
	if err != nil {
		panic(err)
	}
	if write_length != len(value) {
		panic(fmt.Errorf("bad write length when writing []byte, expected %d, written %d", len(value), write_length))
	}
}

func (c FfiConverterBytes) Lift(rb RustBufferI) []byte {
	return LiftFromRustBuffer[[]byte](c, rb)
}

func (c FfiConverterBytes) Read(reader io.Reader) []byte {
	length := readInt32(reader)
	buffer := make([]byte, length)
	read_length, err := reader.Read(buffer)
	if err != nil && err != io.EOF {
		panic(err)
	}
	if read_length != int(length) {
		panic(fmt.Errorf("bad read length when reading []byte, expected %d, read %d", length, read_length))
	}
	return buffer
}

type FfiDestroyerBytes struct{}

func (FfiDestroyerBytes) Destroy(_ []byte) {}

// Below is an implementation of synchronization requirements outlined in the link.
// https://github.com/mozilla/uniffi-rs/blob/0dc031132d9493ca812c3af6e7dd60ad2ea95bf0/uniffi_bindgen/src/bindings/kotlin/templates/ObjectRuntime.kt#L31

type FfiObject struct {
	pointer       unsafe.Pointer
	callCounter   atomic.Int64
	cloneFunction func(unsafe.Pointer, *C.RustCallStatus) unsafe.Pointer
	freeFunction  func(unsafe.Pointer, *C.RustCallStatus)
	destroyed     atomic.Bool
}

func newFfiObject(
	pointer unsafe.Pointer,
	cloneFunction func(unsafe.Pointer, *C.RustCallStatus) unsafe.Pointer,
	freeFunction func(unsafe.Pointer, *C.RustCallStatus),
) FfiObject {
	return FfiObject{
		pointer:       pointer,
		cloneFunction: cloneFunction,
		freeFunction:  freeFunction,
	}
}

func (ffiObject *FfiObject) incrementPointer(debugName string) unsafe.Pointer {
	for {
		counter := ffiObject.callCounter.Load()
		if counter <= -1 {
			panic(fmt.Errorf("%v object has already been destroyed", debugName))
		}
		if counter == math.MaxInt64 {
			panic(fmt.Errorf("%v object call counter would overflow", debugName))
		}
		if ffiObject.callCounter.CompareAndSwap(counter, counter+1) {
			break
		}
	}

	return rustCall(func(status *C.RustCallStatus) unsafe.Pointer {
		return ffiObject.cloneFunction(ffiObject.pointer, status)
	})
}

func (ffiObject *FfiObject) decrementPointer() {
	if ffiObject.callCounter.Add(-1) == -1 {
		ffiObject.freeRustArcPtr()
	}
}

func (ffiObject *FfiObject) destroy() {
	if ffiObject.destroyed.CompareAndSwap(false, true) {
		if ffiObject.callCounter.Add(-1) == -1 {
			ffiObject.freeRustArcPtr()
		}
	}
}

func (ffiObject *FfiObject) freeRustArcPtr() {
	rustCall(func(status *C.RustCallStatus) int32 {
		ffiObject.freeFunction(ffiObject.pointer, status)
		return 0
	})
}

// A single exit-node client. `start()` connects and registers; a background
// supervisor keeps the connection up until `stop()`. Stores nothing itself —
// a device id, if the host keeps one, is passed in and only carried.
type ProxyClientInterface interface {
	// The gateway-assigned `account:instance-id`, or null until connected.
	ClientKey() *string
	Connected() bool
	GatewayId() *string
	// Connect and register. Resolves once online; errors on timeout.
	Start() error
	// Blocking `start`, for bindings without async support (e.g. Ruby). Blocks
	// the caller until connected (or the start timeout).
	StartBlocking() error
	State() ClientState
	// Close the connection and stop reconnecting.
	Stop() error
	// Blocking `stop`.
	StopBlocking() error
}

// A single exit-node client. `start()` connects and registers; a background
// supervisor keeps the connection up until `stop()`. Stores nothing itself —
// a device id, if the host keeps one, is passed in and only carried.
type ProxyClient struct {
	ffiObject FfiObject
}

func NewProxyClient(config ProxyConfig) (*ProxyClient, error) {
	_uniffiRV, _uniffiErr := rustCallWithError[ProxyError](FfiConverterProxyError{}, func(_uniffiStatus *C.RustCallStatus) unsafe.Pointer {
		return C.uniffi_meerkly_fn_constructor_proxyclient_new(FfiConverterProxyConfigINSTANCE.Lower(config), _uniffiStatus)
	})
	if _uniffiErr != nil {
		var _uniffiDefaultValue *ProxyClient
		return _uniffiDefaultValue, _uniffiErr
	} else {
		return FfiConverterProxyClientINSTANCE.Lift(_uniffiRV), nil
	}
}

// The gateway-assigned `account:instance-id`, or null until connected.
func (_self *ProxyClient) ClientKey() *string {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterOptionalStringINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) RustBufferI {
		return GoRustBuffer{
			inner: C.uniffi_meerkly_fn_method_proxyclient_client_key(
				_pointer, _uniffiStatus),
		}
	}))
}

func (_self *ProxyClient) Connected() bool {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterBoolINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) C.int8_t {
		return C.uniffi_meerkly_fn_method_proxyclient_connected(
			_pointer, _uniffiStatus)
	}))
}

func (_self *ProxyClient) GatewayId() *string {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterOptionalStringINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) RustBufferI {
		return GoRustBuffer{
			inner: C.uniffi_meerkly_fn_method_proxyclient_gateway_id(
				_pointer, _uniffiStatus),
		}
	}))
}

// Connect and register. Resolves once online; errors on timeout.
func (_self *ProxyClient) Start() error {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	_, err := uniffiRustCallAsync[ProxyError](
		FfiConverterProxyErrorINSTANCE,
		// completeFn
		func(handle C.uint64_t, status *C.RustCallStatus) struct{} {
			C.ffi_meerkly_rust_future_complete_void(handle, status)
			return struct{}{}
		},
		// liftFn
		func(_ struct{}) struct{} { return struct{}{} },
		C.uniffi_meerkly_fn_method_proxyclient_start(
			_pointer),
		// pollFn
		func(handle C.uint64_t, continuation C.UniffiRustFutureContinuationCallback, data C.uint64_t) {
			C.ffi_meerkly_rust_future_poll_void(handle, continuation, data)
		},
		// freeFn
		func(handle C.uint64_t) {
			C.ffi_meerkly_rust_future_free_void(handle)
		},
	)

	if err != nil {
		return err
	}
	return nil
}

// Blocking `start`, for bindings without async support (e.g. Ruby). Blocks
// the caller until connected (or the start timeout).
func (_self *ProxyClient) StartBlocking() error {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	_, _uniffiErr := rustCallWithError[ProxyError](FfiConverterProxyError{}, func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_meerkly_fn_method_proxyclient_start_blocking(
			_pointer, _uniffiStatus)
		return false
	})
	return _uniffiErr.AsError()
}

func (_self *ProxyClient) State() ClientState {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	return FfiConverterClientStateINSTANCE.Lift(rustCall(func(_uniffiStatus *C.RustCallStatus) RustBufferI {
		return GoRustBuffer{
			inner: C.uniffi_meerkly_fn_method_proxyclient_state(
				_pointer, _uniffiStatus),
		}
	}))
}

// Close the connection and stop reconnecting.
func (_self *ProxyClient) Stop() error {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	_, err := uniffiRustCallAsync[ProxyError](
		FfiConverterProxyErrorINSTANCE,
		// completeFn
		func(handle C.uint64_t, status *C.RustCallStatus) struct{} {
			C.ffi_meerkly_rust_future_complete_void(handle, status)
			return struct{}{}
		},
		// liftFn
		func(_ struct{}) struct{} { return struct{}{} },
		C.uniffi_meerkly_fn_method_proxyclient_stop(
			_pointer),
		// pollFn
		func(handle C.uint64_t, continuation C.UniffiRustFutureContinuationCallback, data C.uint64_t) {
			C.ffi_meerkly_rust_future_poll_void(handle, continuation, data)
		},
		// freeFn
		func(handle C.uint64_t) {
			C.ffi_meerkly_rust_future_free_void(handle)
		},
	)

	if err != nil {
		return err
	}
	return nil
}

// Blocking `stop`.
func (_self *ProxyClient) StopBlocking() error {
	_pointer := _self.ffiObject.incrementPointer("*ProxyClient")
	defer _self.ffiObject.decrementPointer()
	_, _uniffiErr := rustCallWithError[ProxyError](FfiConverterProxyError{}, func(_uniffiStatus *C.RustCallStatus) bool {
		C.uniffi_meerkly_fn_method_proxyclient_stop_blocking(
			_pointer, _uniffiStatus)
		return false
	})
	return _uniffiErr.AsError()
}
func (object *ProxyClient) Destroy() {
	runtime.SetFinalizer(object, nil)
	object.ffiObject.destroy()
}

type FfiConverterProxyClient struct{}

var FfiConverterProxyClientINSTANCE = FfiConverterProxyClient{}

func (c FfiConverterProxyClient) Lift(pointer unsafe.Pointer) *ProxyClient {
	result := &ProxyClient{
		newFfiObject(
			pointer,
			func(pointer unsafe.Pointer, status *C.RustCallStatus) unsafe.Pointer {
				return C.uniffi_meerkly_fn_clone_proxyclient(pointer, status)
			},
			func(pointer unsafe.Pointer, status *C.RustCallStatus) {
				C.uniffi_meerkly_fn_free_proxyclient(pointer, status)
			},
		),
	}
	runtime.SetFinalizer(result, (*ProxyClient).Destroy)
	return result
}

func (c FfiConverterProxyClient) Read(reader io.Reader) *ProxyClient {
	return c.Lift(unsafe.Pointer(uintptr(readUint64(reader))))
}

func (c FfiConverterProxyClient) Lower(value *ProxyClient) unsafe.Pointer {
	// TODO: this is bad - all synchronization from ObjectRuntime.go is discarded here,
	// because the pointer will be decremented immediately after this function returns,
	// and someone will be left holding onto a non-locked pointer.
	pointer := value.ffiObject.incrementPointer("*ProxyClient")
	defer value.ffiObject.decrementPointer()
	return pointer

}

func (c FfiConverterProxyClient) Write(writer io.Writer, value *ProxyClient) {
	writeUint64(writer, uint64(uintptr(c.Lower(value))))
}

type FfiDestroyerProxyClient struct{}

func (_ FfiDestroyerProxyClient) Destroy(value *ProxyClient) {
	value.Destroy()
}

// Options for [`ProxyClient::new`]. In production a host passes only
// `publisherId`; the gateway address and CA are development overrides.
type ProxyConfig struct {
	PublisherId      string
	GatewayAddresses []string
	CaCertPath       *string
	CaCertPem        *[]byte
	StartTimeoutMs   *uint32
	ConnectTimeoutMs *uint32
	// Stable id for this machine, if the host keeps one. The SDK never mints or
	// stores it — only the host knows where an id belongs on its platform.
	DeviceId *string
	// A human name for this machine, conventionally its hostname.
	DeviceName *string
	// Which binding is calling: "python", "ruby", "go", "swift", "kotlin".
	// Unlike the node binding, this crate cannot detect it — uniffi generates
	// all five from this one crate — so a host that wants its language
	// attributed correctly sets it here. Defaults to the SDK's own default.
	Sdk *string
	// The host application, e.g. "my-app/2.1.0".
	App *string
}

func (r *ProxyConfig) Destroy() {
	FfiDestroyerString{}.Destroy(r.PublisherId)
	FfiDestroyerSequenceString{}.Destroy(r.GatewayAddresses)
	FfiDestroyerOptionalString{}.Destroy(r.CaCertPath)
	FfiDestroyerOptionalBytes{}.Destroy(r.CaCertPem)
	FfiDestroyerOptionalUint32{}.Destroy(r.StartTimeoutMs)
	FfiDestroyerOptionalUint32{}.Destroy(r.ConnectTimeoutMs)
	FfiDestroyerOptionalString{}.Destroy(r.DeviceId)
	FfiDestroyerOptionalString{}.Destroy(r.DeviceName)
	FfiDestroyerOptionalString{}.Destroy(r.Sdk)
	FfiDestroyerOptionalString{}.Destroy(r.App)
}

type FfiConverterProxyConfig struct{}

var FfiConverterProxyConfigINSTANCE = FfiConverterProxyConfig{}

func (c FfiConverterProxyConfig) Lift(rb RustBufferI) ProxyConfig {
	return LiftFromRustBuffer[ProxyConfig](c, rb)
}

func (c FfiConverterProxyConfig) Read(reader io.Reader) ProxyConfig {
	return ProxyConfig{
		FfiConverterStringINSTANCE.Read(reader),
		FfiConverterSequenceStringINSTANCE.Read(reader),
		FfiConverterOptionalStringINSTANCE.Read(reader),
		FfiConverterOptionalBytesINSTANCE.Read(reader),
		FfiConverterOptionalUint32INSTANCE.Read(reader),
		FfiConverterOptionalUint32INSTANCE.Read(reader),
		FfiConverterOptionalStringINSTANCE.Read(reader),
		FfiConverterOptionalStringINSTANCE.Read(reader),
		FfiConverterOptionalStringINSTANCE.Read(reader),
		FfiConverterOptionalStringINSTANCE.Read(reader),
	}
}

func (c FfiConverterProxyConfig) Lower(value ProxyConfig) C.RustBuffer {
	return LowerIntoRustBuffer[ProxyConfig](c, value)
}

func (c FfiConverterProxyConfig) Write(writer io.Writer, value ProxyConfig) {
	FfiConverterStringINSTANCE.Write(writer, value.PublisherId)
	FfiConverterSequenceStringINSTANCE.Write(writer, value.GatewayAddresses)
	FfiConverterOptionalStringINSTANCE.Write(writer, value.CaCertPath)
	FfiConverterOptionalBytesINSTANCE.Write(writer, value.CaCertPem)
	FfiConverterOptionalUint32INSTANCE.Write(writer, value.StartTimeoutMs)
	FfiConverterOptionalUint32INSTANCE.Write(writer, value.ConnectTimeoutMs)
	FfiConverterOptionalStringINSTANCE.Write(writer, value.DeviceId)
	FfiConverterOptionalStringINSTANCE.Write(writer, value.DeviceName)
	FfiConverterOptionalStringINSTANCE.Write(writer, value.Sdk)
	FfiConverterOptionalStringINSTANCE.Write(writer, value.App)
}

type FfiDestroyerProxyConfig struct{}

func (_ FfiDestroyerProxyConfig) Destroy(value ProxyConfig) {
	value.Destroy()
}

// What the client is currently doing.
type ClientState uint

const (
	ClientStateIdle       ClientState = 1
	ClientStateConnecting ClientState = 2
	ClientStateConnected  ClientState = 3
	ClientStateStopped    ClientState = 4
)

type FfiConverterClientState struct{}

var FfiConverterClientStateINSTANCE = FfiConverterClientState{}

func (c FfiConverterClientState) Lift(rb RustBufferI) ClientState {
	return LiftFromRustBuffer[ClientState](c, rb)
}

func (c FfiConverterClientState) Lower(value ClientState) C.RustBuffer {
	return LowerIntoRustBuffer[ClientState](c, value)
}
func (FfiConverterClientState) Read(reader io.Reader) ClientState {
	id := readInt32(reader)
	return ClientState(id)
}

func (FfiConverterClientState) Write(writer io.Writer, value ClientState) {
	writeInt32(writer, int32(value))
}

type FfiDestroyerClientState struct{}

func (_ FfiDestroyerClientState) Destroy(value ClientState) {
}

// The single error every fallible SDK call returns.
//
// The field is `reason`, not `message`, and that is not a style choice. uniffi
// generates the Kotlin variant as a subclass of `kotlin.Exception` with an
// `override val message`, so a field literally named `message` collides with
// `Throwable.message` and the generated Kotlin does not compile. Upstream's
// answer is to not name an error field `message`. Nothing else reads the name,
// and Kotlin callers still get the text through `e.message` as usual.
type ProxyError struct {
	err error
}

// Convience method to turn *ProxyError into error
// Avoiding treating nil pointer as non nil error interface
func (err *ProxyError) AsError() error {
	if err == nil {
		return nil
	} else {
		return err
	}
}

func (err ProxyError) Error() string {
	return fmt.Sprintf("ProxyError: %s", err.err.Error())
}

func (err ProxyError) Unwrap() error {
	return err.err
}

// Err* are used for checking error type with `errors.Is`
var ErrProxyErrorFailed = fmt.Errorf("ProxyErrorFailed")

// Variant structs
type ProxyErrorFailed struct {
	Reason string
}

func NewProxyErrorFailed(
	reason string,
) *ProxyError {
	return &ProxyError{err: &ProxyErrorFailed{
		Reason: reason}}
}

func (e ProxyErrorFailed) destroy() {
	FfiDestroyerString{}.Destroy(e.Reason)
}

func (err ProxyErrorFailed) Error() string {
	return fmt.Sprint("Failed",
		": ",

		"Reason=",
		err.Reason,
	)
}

func (self ProxyErrorFailed) Is(target error) bool {
	return target == ErrProxyErrorFailed
}

type FfiConverterProxyError struct{}

var FfiConverterProxyErrorINSTANCE = FfiConverterProxyError{}

func (c FfiConverterProxyError) Lift(eb RustBufferI) *ProxyError {
	return LiftFromRustBuffer[*ProxyError](c, eb)
}

func (c FfiConverterProxyError) Lower(value *ProxyError) C.RustBuffer {
	return LowerIntoRustBuffer[*ProxyError](c, value)
}

func (c FfiConverterProxyError) Read(reader io.Reader) *ProxyError {
	errorID := readUint32(reader)

	switch errorID {
	case 1:
		return &ProxyError{&ProxyErrorFailed{
			Reason: FfiConverterStringINSTANCE.Read(reader),
		}}
	default:
		panic(fmt.Sprintf("Unknown error code %d in FfiConverterProxyError.Read()", errorID))
	}
}

func (c FfiConverterProxyError) Write(writer io.Writer, value *ProxyError) {
	switch variantValue := value.err.(type) {
	case *ProxyErrorFailed:
		writeInt32(writer, 1)
		FfiConverterStringINSTANCE.Write(writer, variantValue.Reason)
	default:
		_ = variantValue
		panic(fmt.Sprintf("invalid error value `%v` in FfiConverterProxyError.Write", value))
	}
}

type FfiDestroyerProxyError struct{}

func (_ FfiDestroyerProxyError) Destroy(value *ProxyError) {
	switch variantValue := value.err.(type) {
	case ProxyErrorFailed:
		variantValue.destroy()
	default:
		_ = variantValue
		panic(fmt.Sprintf("invalid error value `%v` in FfiDestroyerProxyError.Destroy", value))
	}
}

type FfiConverterOptionalUint32 struct{}

var FfiConverterOptionalUint32INSTANCE = FfiConverterOptionalUint32{}

func (c FfiConverterOptionalUint32) Lift(rb RustBufferI) *uint32 {
	return LiftFromRustBuffer[*uint32](c, rb)
}

func (_ FfiConverterOptionalUint32) Read(reader io.Reader) *uint32 {
	if readInt8(reader) == 0 {
		return nil
	}
	temp := FfiConverterUint32INSTANCE.Read(reader)
	return &temp
}

func (c FfiConverterOptionalUint32) Lower(value *uint32) C.RustBuffer {
	return LowerIntoRustBuffer[*uint32](c, value)
}

func (_ FfiConverterOptionalUint32) Write(writer io.Writer, value *uint32) {
	if value == nil {
		writeInt8(writer, 0)
	} else {
		writeInt8(writer, 1)
		FfiConverterUint32INSTANCE.Write(writer, *value)
	}
}

type FfiDestroyerOptionalUint32 struct{}

func (_ FfiDestroyerOptionalUint32) Destroy(value *uint32) {
	if value != nil {
		FfiDestroyerUint32{}.Destroy(*value)
	}
}

type FfiConverterOptionalString struct{}

var FfiConverterOptionalStringINSTANCE = FfiConverterOptionalString{}

func (c FfiConverterOptionalString) Lift(rb RustBufferI) *string {
	return LiftFromRustBuffer[*string](c, rb)
}

func (_ FfiConverterOptionalString) Read(reader io.Reader) *string {
	if readInt8(reader) == 0 {
		return nil
	}
	temp := FfiConverterStringINSTANCE.Read(reader)
	return &temp
}

func (c FfiConverterOptionalString) Lower(value *string) C.RustBuffer {
	return LowerIntoRustBuffer[*string](c, value)
}

func (_ FfiConverterOptionalString) Write(writer io.Writer, value *string) {
	if value == nil {
		writeInt8(writer, 0)
	} else {
		writeInt8(writer, 1)
		FfiConverterStringINSTANCE.Write(writer, *value)
	}
}

type FfiDestroyerOptionalString struct{}

func (_ FfiDestroyerOptionalString) Destroy(value *string) {
	if value != nil {
		FfiDestroyerString{}.Destroy(*value)
	}
}

type FfiConverterOptionalBytes struct{}

var FfiConverterOptionalBytesINSTANCE = FfiConverterOptionalBytes{}

func (c FfiConverterOptionalBytes) Lift(rb RustBufferI) *[]byte {
	return LiftFromRustBuffer[*[]byte](c, rb)
}

func (_ FfiConverterOptionalBytes) Read(reader io.Reader) *[]byte {
	if readInt8(reader) == 0 {
		return nil
	}
	temp := FfiConverterBytesINSTANCE.Read(reader)
	return &temp
}

func (c FfiConverterOptionalBytes) Lower(value *[]byte) C.RustBuffer {
	return LowerIntoRustBuffer[*[]byte](c, value)
}

func (_ FfiConverterOptionalBytes) Write(writer io.Writer, value *[]byte) {
	if value == nil {
		writeInt8(writer, 0)
	} else {
		writeInt8(writer, 1)
		FfiConverterBytesINSTANCE.Write(writer, *value)
	}
}

type FfiDestroyerOptionalBytes struct{}

func (_ FfiDestroyerOptionalBytes) Destroy(value *[]byte) {
	if value != nil {
		FfiDestroyerBytes{}.Destroy(*value)
	}
}

type FfiConverterSequenceString struct{}

var FfiConverterSequenceStringINSTANCE = FfiConverterSequenceString{}

func (c FfiConverterSequenceString) Lift(rb RustBufferI) []string {
	return LiftFromRustBuffer[[]string](c, rb)
}

func (c FfiConverterSequenceString) Read(reader io.Reader) []string {
	length := readInt32(reader)
	if length == 0 {
		return nil
	}
	result := make([]string, 0, length)
	for i := int32(0); i < length; i++ {
		result = append(result, FfiConverterStringINSTANCE.Read(reader))
	}
	return result
}

func (c FfiConverterSequenceString) Lower(value []string) C.RustBuffer {
	return LowerIntoRustBuffer[[]string](c, value)
}

func (c FfiConverterSequenceString) Write(writer io.Writer, value []string) {
	if len(value) > math.MaxInt32 {
		panic("[]string is too large to fit into Int32")
	}

	writeInt32(writer, int32(len(value)))
	for _, item := range value {
		FfiConverterStringINSTANCE.Write(writer, item)
	}
}

type FfiDestroyerSequenceString struct{}

func (FfiDestroyerSequenceString) Destroy(sequence []string) {
	for _, value := range sequence {
		FfiDestroyerString{}.Destroy(value)
	}
}

const (
	uniffiRustFuturePollReady      int8 = 0
	uniffiRustFuturePollMaybeReady int8 = 1
)

type rustFuturePollFunc func(C.uint64_t, C.UniffiRustFutureContinuationCallback, C.uint64_t)
type rustFutureCompleteFunc[T any] func(C.uint64_t, *C.RustCallStatus) T
type rustFutureFreeFunc func(C.uint64_t)

//export meerkly_uniffiFutureContinuationCallback
func meerkly_uniffiFutureContinuationCallback(data C.uint64_t, pollResult C.int8_t) {
	h := cgo.Handle(uintptr(data))
	waiter := h.Value().(chan int8)
	waiter <- int8(pollResult)
}

func uniffiRustCallAsync[E any, T any, F any](
	errConverter BufReader[*E],
	completeFunc rustFutureCompleteFunc[F],
	liftFunc func(F) T,
	rustFuture C.uint64_t,
	pollFunc rustFuturePollFunc,
	freeFunc rustFutureFreeFunc,
) (T, *E) {
	defer freeFunc(rustFuture)

	pollResult := int8(-1)
	waiter := make(chan int8, 1)

	chanHandle := cgo.NewHandle(waiter)
	defer chanHandle.Delete()

	for pollResult != uniffiRustFuturePollReady {
		pollFunc(
			rustFuture,
			(C.UniffiRustFutureContinuationCallback)(C.meerkly_uniffiFutureContinuationCallback),
			C.uint64_t(chanHandle),
		)
		pollResult = <-waiter
	}

	var goValue T
	var ffiValue F
	var err *E

	ffiValue, err = rustCallWithError(errConverter, func(status *C.RustCallStatus) F {
		return completeFunc(rustFuture, status)
	})
	if err != nil {
		return goValue, err
	}
	return liftFunc(ffiValue), nil
}

//export meerkly_uniffiFreeGorutine
func meerkly_uniffiFreeGorutine(data C.uint64_t) {
	handle := cgo.Handle(uintptr(data))
	defer handle.Delete()

	guard := handle.Value().(chan struct{})
	guard <- struct{}{}
}
