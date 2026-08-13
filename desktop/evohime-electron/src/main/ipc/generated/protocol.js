// GENERATED FILE - DO NOT EDIT.
// Source: crates/desktop-ipc/proto/evohime.desktop.proto
// Regenerate with: npm run generate:protocol
/*eslint-disable block-scoped-var, id-length, no-control-regex, no-magic-numbers, no-mixed-operators, no-prototype-builtins, no-redeclare, no-shadow, no-var, sort-vars, default-case, jsdoc/require-param*/
import $protobuf from "protobufjs/minimal.js";

// Common aliases
const $Reader = $protobuf.Reader, $Writer = $protobuf.Writer, $util = $protobuf.util;
const $Object = $util.global.Object, $undefined = $util.global.undefined, $Error = $util.global.Error;

// Exported root namespace
const $root = $protobuf.roots["default"] || ($protobuf.roots["default"] = {});

export const evohime = $root.evohime = (() => {

    /**
     * Namespace evohime.
     * @exports evohime
     * @namespace
     */
    const evohime = {};

    evohime.desktop = (function() {

        /**
         * Namespace desktop.
         * @memberof evohime
         * @namespace
         */
        const desktop = {};

        desktop.v1 = (function() {

            /**
             * Namespace v1.
             * @memberof evohime.desktop
             * @namespace
             */
            const v1 = {};

            v1.ProtocolVersion = (function() {

                /**
                 * Properties of a ProtocolVersion.
                 * @typedef {Object} evohime.desktop.v1.ProtocolVersion.$Properties
                 * @property {number|null} [major] ProtocolVersion major
                 * @property {number|null} [minor] ProtocolVersion minor
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ProtocolVersion.
                 * @memberof evohime.desktop.v1
                 * @interface IProtocolVersion
                 * @augments evohime.desktop.v1.ProtocolVersion.$Properties
                 * @deprecated Use evohime.desktop.v1.ProtocolVersion.$Properties instead.
                 */

                /**
                 * Shape of a ProtocolVersion.
                 * @typedef {evohime.desktop.v1.ProtocolVersion.$Properties} evohime.desktop.v1.ProtocolVersion.$Shape
                 */

                /**
                 * Constructs a new ProtocolVersion.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ProtocolVersion.
                 * @constructor
                 * @param {evohime.desktop.v1.ProtocolVersion.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ProtocolVersion = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ProtocolVersion major.
                 * @member {number} major
                 * @memberof evohime.desktop.v1.ProtocolVersion
                 * @instance
                 */
                ProtocolVersion.prototype.major = 0;

                /**
                 * ProtocolVersion minor.
                 * @member {number} minor
                 * @memberof evohime.desktop.v1.ProtocolVersion
                 * @instance
                 */
                ProtocolVersion.prototype.minor = 0;

                /**
                 * Encodes the specified ProtocolVersion message. Does not implicitly {@link evohime.desktop.v1.ProtocolVersion.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ProtocolVersion
                 * @static
                 * @param {evohime.desktop.v1.ProtocolVersion.$Properties} message ProtocolVersion message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ProtocolVersion.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.major != null && $Object.hasOwnProperty.call(message, "major") && message.major !== 0)
                        writer.uint32(/* id 1, wireType 0 =*/8).uint32(message.major);
                    if (message.minor != null && $Object.hasOwnProperty.call(message, "minor") && message.minor !== 0)
                        writer.uint32(/* id 2, wireType 0 =*/16).uint32(message.minor);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ProtocolVersion message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ProtocolVersion
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ProtocolVersion & evohime.desktop.v1.ProtocolVersion.$Shape} ProtocolVersion
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ProtocolVersion.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ProtocolVersion(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.major = value;
                                else
                                    delete message.major;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.minor = value;
                                else
                                    delete message.minor;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ProtocolVersion
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ProtocolVersion
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ProtocolVersion.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ProtocolVersion";
                };

                return ProtocolVersion;
            })();

            v1.ProtocolOffer = (function() {

                /**
                 * Properties of a ProtocolOffer.
                 * @typedef {Object} evohime.desktop.v1.ProtocolOffer.$Properties
                 * @property {evohime.desktop.v1.ProtocolVersion.$Properties|null} [protocol] ProtocolOffer protocol
                 * @property {Array.<string>|null} [capabilities] ProtocolOffer capabilities
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ProtocolOffer.
                 * @memberof evohime.desktop.v1
                 * @interface IProtocolOffer
                 * @augments evohime.desktop.v1.ProtocolOffer.$Properties
                 * @deprecated Use evohime.desktop.v1.ProtocolOffer.$Properties instead.
                 */

                /**
                 * Shape of a ProtocolOffer.
                 * @typedef {evohime.desktop.v1.ProtocolOffer.$Properties} evohime.desktop.v1.ProtocolOffer.$Shape
                 */

                /**
                 * Constructs a new ProtocolOffer.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ProtocolOffer.
                 * @constructor
                 * @param {evohime.desktop.v1.ProtocolOffer.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ProtocolOffer = function (properties) {
                    this.capabilities = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ProtocolOffer protocol.
                 * @member {evohime.desktop.v1.ProtocolVersion.$Properties|null|undefined} protocol
                 * @memberof evohime.desktop.v1.ProtocolOffer
                 * @instance
                 */
                ProtocolOffer.prototype.protocol = null;

                /**
                 * ProtocolOffer capabilities.
                 * @member {Array.<string>} capabilities
                 * @memberof evohime.desktop.v1.ProtocolOffer
                 * @instance
                 */
                ProtocolOffer.prototype.capabilities = $util.emptyArray;

                /**
                 * Encodes the specified ProtocolOffer message. Does not implicitly {@link evohime.desktop.v1.ProtocolOffer.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ProtocolOffer
                 * @static
                 * @param {evohime.desktop.v1.ProtocolOffer.$Properties} message ProtocolOffer message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ProtocolOffer.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.protocol != null && $Object.hasOwnProperty.call(message, "protocol"))
                        $root.evohime.desktop.v1.ProtocolVersion.encode(message.protocol, writer.uint32(/* id 1, wireType 2 =*/10).fork(), _depth + 1).ldelim();
                    if (message.capabilities != null && message.capabilities.length)
                        for (let i = 0; i < message.capabilities.length; ++i)
                            writer.uint32(/* id 2, wireType 2 =*/18).string(message.capabilities[i]);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ProtocolOffer message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ProtocolOffer
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ProtocolOffer & evohime.desktop.v1.ProtocolOffer.$Shape} ProtocolOffer
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ProtocolOffer.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ProtocolOffer(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                message.protocol = $root.evohime.desktop.v1.ProtocolVersion.decode(reader, reader.uint32(), $undefined, _depth + 1, message.protocol);
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.capabilities && message.capabilities.length))
                                    message.capabilities = [];
                                message.capabilities.push(reader.stringVerify());
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ProtocolOffer
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ProtocolOffer
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ProtocolOffer.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ProtocolOffer";
                };

                return ProtocolOffer;
            })();

            v1.Handshake = (function() {

                /**
                 * Properties of a Handshake.
                 * @typedef {Object} evohime.desktop.v1.Handshake.$Properties
                 * @property {evohime.desktop.v1.ProtocolVersion.$Properties|null} [protocol] Handshake protocol
                 * @property {string|null} [clientId] Handshake clientId
                 * @property {string|null} [sessionId] Handshake sessionId
                 * @property {number|null} [sessionEpoch] Handshake sessionEpoch
                 * @property {number|null} [lastEventSequence] Handshake lastEventSequence
                 * @property {Array.<string>|null} [capabilities] Handshake capabilities
                 * @property {string|null} [clientRole] Handshake clientRole
                 * @property {string|null} [nonce] Handshake nonce
                 * @property {string|null} [proof] Handshake proof
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a Handshake.
                 * @memberof evohime.desktop.v1
                 * @interface IHandshake
                 * @augments evohime.desktop.v1.Handshake.$Properties
                 * @deprecated Use evohime.desktop.v1.Handshake.$Properties instead.
                 */

                /**
                 * Shape of a Handshake.
                 * @typedef {evohime.desktop.v1.Handshake.$Properties} evohime.desktop.v1.Handshake.$Shape
                 */

                /**
                 * Constructs a new Handshake.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a Handshake.
                 * @constructor
                 * @param {evohime.desktop.v1.Handshake.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const Handshake = function (properties) {
                    this.capabilities = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * Handshake protocol.
                 * @member {evohime.desktop.v1.ProtocolVersion.$Properties|null|undefined} protocol
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.protocol = null;

                /**
                 * Handshake clientId.
                 * @member {string} clientId
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.clientId = "";

                /**
                 * Handshake sessionId.
                 * @member {string} sessionId
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.sessionId = "";

                /**
                 * Handshake sessionEpoch.
                 * @member {number} sessionEpoch
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.sessionEpoch = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Handshake lastEventSequence.
                 * @member {number} lastEventSequence
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.lastEventSequence = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Handshake capabilities.
                 * @member {Array.<string>} capabilities
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.capabilities = $util.emptyArray;

                /**
                 * Handshake clientRole.
                 * @member {string} clientRole
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.clientRole = "";

                /**
                 * Handshake nonce.
                 * @member {string} nonce
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.nonce = "";

                /**
                 * Handshake proof.
                 * @member {string} proof
                 * @memberof evohime.desktop.v1.Handshake
                 * @instance
                 */
                Handshake.prototype.proof = "";

                /**
                 * Encodes the specified Handshake message. Does not implicitly {@link evohime.desktop.v1.Handshake.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.Handshake
                 * @static
                 * @param {evohime.desktop.v1.Handshake.$Properties} message Handshake message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                Handshake.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.protocol != null && $Object.hasOwnProperty.call(message, "protocol"))
                        $root.evohime.desktop.v1.ProtocolVersion.encode(message.protocol, writer.uint32(/* id 1, wireType 2 =*/10).fork(), _depth + 1).ldelim();
                    if (message.clientId != null && $Object.hasOwnProperty.call(message, "clientId") && message.clientId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.clientId);
                    if (message.sessionId != null && $Object.hasOwnProperty.call(message, "sessionId") && message.sessionId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.sessionId);
                    if (message.sessionEpoch != null && $Object.hasOwnProperty.call(message, "sessionEpoch") && (typeof message.sessionEpoch === "object" ? message.sessionEpoch.low || message.sessionEpoch.high : message.sessionEpoch !== 0))
                        writer.uint32(/* id 4, wireType 0 =*/32).uint64(message.sessionEpoch);
                    if (message.lastEventSequence != null && $Object.hasOwnProperty.call(message, "lastEventSequence") && (typeof message.lastEventSequence === "object" ? message.lastEventSequence.low || message.lastEventSequence.high : message.lastEventSequence !== 0))
                        writer.uint32(/* id 5, wireType 0 =*/40).uint64(message.lastEventSequence);
                    if (message.capabilities != null && message.capabilities.length)
                        for (let i = 0; i < message.capabilities.length; ++i)
                            writer.uint32(/* id 6, wireType 2 =*/50).string(message.capabilities[i]);
                    if (message.clientRole != null && $Object.hasOwnProperty.call(message, "clientRole") && message.clientRole !== "")
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.clientRole);
                    if (message.nonce != null && $Object.hasOwnProperty.call(message, "nonce") && message.nonce !== "")
                        writer.uint32(/* id 8, wireType 2 =*/66).string(message.nonce);
                    if (message.proof != null && $Object.hasOwnProperty.call(message, "proof") && message.proof !== "")
                        writer.uint32(/* id 9, wireType 2 =*/74).string(message.proof);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a Handshake message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.Handshake
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.Handshake & evohime.desktop.v1.Handshake.$Shape} Handshake
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                Handshake.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.Handshake(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                message.protocol = $root.evohime.desktop.v1.ProtocolVersion.decode(reader, reader.uint32(), $undefined, _depth + 1, message.protocol);
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.clientId = value;
                                else
                                    delete message.clientId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.sessionId = value;
                                else
                                    delete message.sessionId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.sessionEpoch = value;
                                else
                                    delete message.sessionEpoch;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.lastEventSequence = value;
                                else
                                    delete message.lastEventSequence;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.capabilities && message.capabilities.length))
                                    message.capabilities = [];
                                message.capabilities.push(reader.stringVerify());
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.clientRole = value;
                                else
                                    delete message.clientRole;
                                continue;
                            }
                        case 8: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.nonce = value;
                                else
                                    delete message.nonce;
                                continue;
                            }
                        case 9: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.proof = value;
                                else
                                    delete message.proof;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for Handshake
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.Handshake
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                Handshake.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.Handshake";
                };

                return Handshake;
            })();

            v1.AuthChallenge = (function() {

                /**
                 * Properties of an AuthChallenge.
                 * @typedef {Object} evohime.desktop.v1.AuthChallenge.$Properties
                 * @property {string|null} [nonce] AuthChallenge nonce
                 * @property {number|null} [expiresAtMs] AuthChallenge expiresAtMs
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an AuthChallenge.
                 * @memberof evohime.desktop.v1
                 * @interface IAuthChallenge
                 * @augments evohime.desktop.v1.AuthChallenge.$Properties
                 * @deprecated Use evohime.desktop.v1.AuthChallenge.$Properties instead.
                 */

                /**
                 * Shape of an AuthChallenge.
                 * @typedef {evohime.desktop.v1.AuthChallenge.$Properties} evohime.desktop.v1.AuthChallenge.$Shape
                 */

                /**
                 * Constructs a new AuthChallenge.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an AuthChallenge.
                 * @constructor
                 * @param {evohime.desktop.v1.AuthChallenge.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const AuthChallenge = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * AuthChallenge nonce.
                 * @member {string} nonce
                 * @memberof evohime.desktop.v1.AuthChallenge
                 * @instance
                 */
                AuthChallenge.prototype.nonce = "";

                /**
                 * AuthChallenge expiresAtMs.
                 * @member {number} expiresAtMs
                 * @memberof evohime.desktop.v1.AuthChallenge
                 * @instance
                 */
                AuthChallenge.prototype.expiresAtMs = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Encodes the specified AuthChallenge message. Does not implicitly {@link evohime.desktop.v1.AuthChallenge.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.AuthChallenge
                 * @static
                 * @param {evohime.desktop.v1.AuthChallenge.$Properties} message AuthChallenge message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                AuthChallenge.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.nonce != null && $Object.hasOwnProperty.call(message, "nonce") && message.nonce !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.nonce);
                    if (message.expiresAtMs != null && $Object.hasOwnProperty.call(message, "expiresAtMs") && (typeof message.expiresAtMs === "object" ? message.expiresAtMs.low || message.expiresAtMs.high : message.expiresAtMs !== 0))
                        writer.uint32(/* id 2, wireType 0 =*/16).uint64(message.expiresAtMs);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an AuthChallenge message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.AuthChallenge
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AuthChallenge & evohime.desktop.v1.AuthChallenge.$Shape} AuthChallenge
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                AuthChallenge.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.AuthChallenge(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.nonce = value;
                                else
                                    delete message.nonce;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.expiresAtMs = value;
                                else
                                    delete message.expiresAtMs;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for AuthChallenge
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.AuthChallenge
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                AuthChallenge.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.AuthChallenge";
                };

                return AuthChallenge;
            })();

            v1.ReplayEvents = (function() {

                /**
                 * Properties of a ReplayEvents.
                 * @typedef {Object} evohime.desktop.v1.ReplayEvents.$Properties
                 * @property {number|null} [afterSequence] ReplayEvents afterSequence
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ReplayEvents.
                 * @memberof evohime.desktop.v1
                 * @interface IReplayEvents
                 * @augments evohime.desktop.v1.ReplayEvents.$Properties
                 * @deprecated Use evohime.desktop.v1.ReplayEvents.$Properties instead.
                 */

                /**
                 * Shape of a ReplayEvents.
                 * @typedef {evohime.desktop.v1.ReplayEvents.$Properties} evohime.desktop.v1.ReplayEvents.$Shape
                 */

                /**
                 * Constructs a new ReplayEvents.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ReplayEvents.
                 * @constructor
                 * @param {evohime.desktop.v1.ReplayEvents.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ReplayEvents = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ReplayEvents afterSequence.
                 * @member {number} afterSequence
                 * @memberof evohime.desktop.v1.ReplayEvents
                 * @instance
                 */
                ReplayEvents.prototype.afterSequence = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Encodes the specified ReplayEvents message. Does not implicitly {@link evohime.desktop.v1.ReplayEvents.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ReplayEvents
                 * @static
                 * @param {evohime.desktop.v1.ReplayEvents.$Properties} message ReplayEvents message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ReplayEvents.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.afterSequence != null && $Object.hasOwnProperty.call(message, "afterSequence") && (typeof message.afterSequence === "object" ? message.afterSequence.low || message.afterSequence.high : message.afterSequence !== 0))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.afterSequence);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ReplayEvents message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ReplayEvents
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReplayEvents & evohime.desktop.v1.ReplayEvents.$Shape} ReplayEvents
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ReplayEvents.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ReplayEvents(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.afterSequence = value;
                                else
                                    delete message.afterSequence;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ReplayEvents
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ReplayEvents
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ReplayEvents.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ReplayEvents";
                };

                return ReplayEvents;
            })();

            v1.ResyncRequest = (function() {

                /**
                 * Properties of a ResyncRequest.
                 * @typedef {Object} evohime.desktop.v1.ResyncRequest.$Properties
                 * @property {number|null} [afterSequence] ResyncRequest afterSequence
                 * @property {number|null} [maxEvents] ResyncRequest maxEvents
                 * @property {boolean|null} [includeFullSnapshot] ResyncRequest includeFullSnapshot
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ResyncRequest.
                 * @memberof evohime.desktop.v1
                 * @interface IResyncRequest
                 * @augments evohime.desktop.v1.ResyncRequest.$Properties
                 * @deprecated Use evohime.desktop.v1.ResyncRequest.$Properties instead.
                 */

                /**
                 * Shape of a ResyncRequest.
                 * @typedef {evohime.desktop.v1.ResyncRequest.$Properties} evohime.desktop.v1.ResyncRequest.$Shape
                 */

                /**
                 * Constructs a new ResyncRequest.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ResyncRequest.
                 * @constructor
                 * @param {evohime.desktop.v1.ResyncRequest.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ResyncRequest = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ResyncRequest afterSequence.
                 * @member {number} afterSequence
                 * @memberof evohime.desktop.v1.ResyncRequest
                 * @instance
                 */
                ResyncRequest.prototype.afterSequence = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * ResyncRequest maxEvents.
                 * @member {number} maxEvents
                 * @memberof evohime.desktop.v1.ResyncRequest
                 * @instance
                 */
                ResyncRequest.prototype.maxEvents = 0;

                /**
                 * ResyncRequest includeFullSnapshot.
                 * @member {boolean} includeFullSnapshot
                 * @memberof evohime.desktop.v1.ResyncRequest
                 * @instance
                 */
                ResyncRequest.prototype.includeFullSnapshot = false;

                /**
                 * Encodes the specified ResyncRequest message. Does not implicitly {@link evohime.desktop.v1.ResyncRequest.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ResyncRequest
                 * @static
                 * @param {evohime.desktop.v1.ResyncRequest.$Properties} message ResyncRequest message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ResyncRequest.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.afterSequence != null && $Object.hasOwnProperty.call(message, "afterSequence") && (typeof message.afterSequence === "object" ? message.afterSequence.low || message.afterSequence.high : message.afterSequence !== 0))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.afterSequence);
                    if (message.maxEvents != null && $Object.hasOwnProperty.call(message, "maxEvents") && message.maxEvents !== 0)
                        writer.uint32(/* id 2, wireType 0 =*/16).uint32(message.maxEvents);
                    if (message.includeFullSnapshot != null && $Object.hasOwnProperty.call(message, "includeFullSnapshot") && message.includeFullSnapshot !== false)
                        writer.uint32(/* id 3, wireType 0 =*/24).bool(message.includeFullSnapshot);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ResyncRequest message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ResyncRequest
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ResyncRequest & evohime.desktop.v1.ResyncRequest.$Shape} ResyncRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ResyncRequest.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ResyncRequest(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.afterSequence = value;
                                else
                                    delete message.afterSequence;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxEvents = value;
                                else
                                    delete message.maxEvents;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.bool())
                                    message.includeFullSnapshot = value;
                                else
                                    delete message.includeFullSnapshot;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ResyncRequest
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ResyncRequest
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ResyncRequest.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ResyncRequest";
                };

                return ResyncRequest;
            })();

            v1.ModelConfigRequest = (function() {

                /**
                 * Properties of a ModelConfigRequest.
                 * @typedef {Object} evohime.desktop.v1.ModelConfigRequest.$Properties
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ModelConfigRequest.
                 * @memberof evohime.desktop.v1
                 * @interface IModelConfigRequest
                 * @augments evohime.desktop.v1.ModelConfigRequest.$Properties
                 * @deprecated Use evohime.desktop.v1.ModelConfigRequest.$Properties instead.
                 */

                /**
                 * Shape of a ModelConfigRequest.
                 * @typedef {evohime.desktop.v1.ModelConfigRequest.$Properties} evohime.desktop.v1.ModelConfigRequest.$Shape
                 */

                /**
                 * Constructs a new ModelConfigRequest.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ModelConfigRequest.
                 * @constructor
                 * @param {evohime.desktop.v1.ModelConfigRequest.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ModelConfigRequest = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * Encodes the specified ModelConfigRequest message. Does not implicitly {@link evohime.desktop.v1.ModelConfigRequest.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ModelConfigRequest
                 * @static
                 * @param {evohime.desktop.v1.ModelConfigRequest.$Properties} message ModelConfigRequest message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ModelConfigRequest.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ModelConfigRequest message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ModelConfigRequest
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ModelConfigRequest & evohime.desktop.v1.ModelConfigRequest.$Shape} ModelConfigRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ModelConfigRequest.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ModelConfigRequest();
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        reader.skipType(tag & 7, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ModelConfigRequest
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ModelConfigRequest
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ModelConfigRequest.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ModelConfigRequest";
                };

                return ModelConfigRequest;
            })();

            v1.ModelCatalogRequest = (function() {

                /**
                 * Properties of a ModelCatalogRequest.
                 * @typedef {Object} evohime.desktop.v1.ModelCatalogRequest.$Properties
                 * @property {string|null} [mode] ModelCatalogRequest mode
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ModelCatalogRequest.
                 * @memberof evohime.desktop.v1
                 * @interface IModelCatalogRequest
                 * @augments evohime.desktop.v1.ModelCatalogRequest.$Properties
                 * @deprecated Use evohime.desktop.v1.ModelCatalogRequest.$Properties instead.
                 */

                /**
                 * Shape of a ModelCatalogRequest.
                 * @typedef {evohime.desktop.v1.ModelCatalogRequest.$Properties} evohime.desktop.v1.ModelCatalogRequest.$Shape
                 */

                /**
                 * Constructs a new ModelCatalogRequest.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ModelCatalogRequest.
                 * @constructor
                 * @param {evohime.desktop.v1.ModelCatalogRequest.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ModelCatalogRequest = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ModelCatalogRequest mode.
                 * @member {string} mode
                 * @memberof evohime.desktop.v1.ModelCatalogRequest
                 * @instance
                 */
                ModelCatalogRequest.prototype.mode = "";

                /**
                 * Encodes the specified ModelCatalogRequest message. Does not implicitly {@link evohime.desktop.v1.ModelCatalogRequest.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ModelCatalogRequest
                 * @static
                 * @param {evohime.desktop.v1.ModelCatalogRequest.$Properties} message ModelCatalogRequest message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ModelCatalogRequest.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.mode != null && $Object.hasOwnProperty.call(message, "mode") && message.mode !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.mode);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ModelCatalogRequest message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ModelCatalogRequest
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ModelCatalogRequest & evohime.desktop.v1.ModelCatalogRequest.$Shape} ModelCatalogRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ModelCatalogRequest.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ModelCatalogRequest(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.mode = value;
                                else
                                    delete message.mode;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ModelCatalogRequest
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ModelCatalogRequest
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ModelCatalogRequest.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ModelCatalogRequest";
                };

                return ModelCatalogRequest;
            })();

            v1.SelectModelRequest = (function() {

                /**
                 * Properties of a SelectModelRequest.
                 * @typedef {Object} evohime.desktop.v1.SelectModelRequest.$Properties
                 * @property {string|null} [model] SelectModelRequest model
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a SelectModelRequest.
                 * @memberof evohime.desktop.v1
                 * @interface ISelectModelRequest
                 * @augments evohime.desktop.v1.SelectModelRequest.$Properties
                 * @deprecated Use evohime.desktop.v1.SelectModelRequest.$Properties instead.
                 */

                /**
                 * Shape of a SelectModelRequest.
                 * @typedef {evohime.desktop.v1.SelectModelRequest.$Properties} evohime.desktop.v1.SelectModelRequest.$Shape
                 */

                /**
                 * Constructs a new SelectModelRequest.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a SelectModelRequest.
                 * @constructor
                 * @param {evohime.desktop.v1.SelectModelRequest.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const SelectModelRequest = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * SelectModelRequest model.
                 * @member {string} model
                 * @memberof evohime.desktop.v1.SelectModelRequest
                 * @instance
                 */
                SelectModelRequest.prototype.model = "";

                /**
                 * Encodes the specified SelectModelRequest message. Does not implicitly {@link evohime.desktop.v1.SelectModelRequest.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.SelectModelRequest
                 * @static
                 * @param {evohime.desktop.v1.SelectModelRequest.$Properties} message SelectModelRequest message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                SelectModelRequest.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.model != null && $Object.hasOwnProperty.call(message, "model") && message.model !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.model);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a SelectModelRequest message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.SelectModelRequest
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SelectModelRequest & evohime.desktop.v1.SelectModelRequest.$Shape} SelectModelRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                SelectModelRequest.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.SelectModelRequest(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.model = value;
                                else
                                    delete message.model;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for SelectModelRequest
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.SelectModelRequest
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                SelectModelRequest.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.SelectModelRequest";
                };

                return SelectModelRequest;
            })();

            v1.PermissionModeRequest = (function() {

                /**
                 * Properties of a PermissionModeRequest.
                 * @typedef {Object} evohime.desktop.v1.PermissionModeRequest.$Properties
                 * @property {string|null} [mode] PermissionModeRequest mode
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a PermissionModeRequest.
                 * @memberof evohime.desktop.v1
                 * @interface IPermissionModeRequest
                 * @augments evohime.desktop.v1.PermissionModeRequest.$Properties
                 * @deprecated Use evohime.desktop.v1.PermissionModeRequest.$Properties instead.
                 */

                /**
                 * Shape of a PermissionModeRequest.
                 * @typedef {evohime.desktop.v1.PermissionModeRequest.$Properties} evohime.desktop.v1.PermissionModeRequest.$Shape
                 */

                /**
                 * Constructs a new PermissionModeRequest.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a PermissionModeRequest.
                 * @constructor
                 * @param {evohime.desktop.v1.PermissionModeRequest.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const PermissionModeRequest = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * PermissionModeRequest mode.
                 * @member {string} mode
                 * @memberof evohime.desktop.v1.PermissionModeRequest
                 * @instance
                 */
                PermissionModeRequest.prototype.mode = "";

                /**
                 * Encodes the specified PermissionModeRequest message. Does not implicitly {@link evohime.desktop.v1.PermissionModeRequest.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.PermissionModeRequest
                 * @static
                 * @param {evohime.desktop.v1.PermissionModeRequest.$Properties} message PermissionModeRequest message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                PermissionModeRequest.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.mode != null && $Object.hasOwnProperty.call(message, "mode") && message.mode !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.mode);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a PermissionModeRequest message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.PermissionModeRequest
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PermissionModeRequest & evohime.desktop.v1.PermissionModeRequest.$Shape} PermissionModeRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                PermissionModeRequest.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.PermissionModeRequest(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.mode = value;
                                else
                                    delete message.mode;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for PermissionModeRequest
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.PermissionModeRequest
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                PermissionModeRequest.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.PermissionModeRequest";
                };

                return PermissionModeRequest;
            })();

            /**
             * TaskStatus enum.
             * @name evohime.desktop.v1.TaskStatus
             * @enum {number}
             * @property {number} TASK_STATUS_UNKNOWN=0 TASK_STATUS_UNKNOWN value
             * @property {number} TASK_STATUS_BACKLOG=1 TASK_STATUS_BACKLOG value
             * @property {number} TASK_STATUS_READY=2 TASK_STATUS_READY value
             * @property {number} TASK_STATUS_IN_PROGRESS=3 TASK_STATUS_IN_PROGRESS value
             * @property {number} TASK_STATUS_DONE=4 TASK_STATUS_DONE value
             */
            v1.TaskStatus = (function() {
                const valuesById = $Object.create(null), values = $Object.create(valuesById);
                values[valuesById[0] = "TASK_STATUS_UNKNOWN"] = 0;
                values[valuesById[1] = "TASK_STATUS_BACKLOG"] = 1;
                values[valuesById[2] = "TASK_STATUS_READY"] = 2;
                values[valuesById[3] = "TASK_STATUS_IN_PROGRESS"] = 3;
                values[valuesById[4] = "TASK_STATUS_DONE"] = 4;
                return values;
            })();

            v1.CreateProject = (function() {

                /**
                 * Properties of a CreateProject.
                 * @typedef {Object} evohime.desktop.v1.CreateProject.$Properties
                 * @property {string|null} [projectId] CreateProject projectId
                 * @property {string|null} [title] CreateProject title
                 * @property {string|null} [workspacePath] CreateProject workspacePath
                 * @property {string|null} [sourceRef] CreateProject sourceRef
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a CreateProject.
                 * @memberof evohime.desktop.v1
                 * @interface ICreateProject
                 * @augments evohime.desktop.v1.CreateProject.$Properties
                 * @deprecated Use evohime.desktop.v1.CreateProject.$Properties instead.
                 */

                /**
                 * Shape of a CreateProject.
                 * @typedef {evohime.desktop.v1.CreateProject.$Properties} evohime.desktop.v1.CreateProject.$Shape
                 */

                /**
                 * Constructs a new CreateProject.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a CreateProject.
                 * @constructor
                 * @param {evohime.desktop.v1.CreateProject.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const CreateProject = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * CreateProject projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.CreateProject
                 * @instance
                 */
                CreateProject.prototype.projectId = "";

                /**
                 * CreateProject title.
                 * @member {string} title
                 * @memberof evohime.desktop.v1.CreateProject
                 * @instance
                 */
                CreateProject.prototype.title = "";

                /**
                 * CreateProject workspacePath.
                 * @member {string} workspacePath
                 * @memberof evohime.desktop.v1.CreateProject
                 * @instance
                 */
                CreateProject.prototype.workspacePath = "";

                /**
                 * CreateProject sourceRef.
                 * @member {string} sourceRef
                 * @memberof evohime.desktop.v1.CreateProject
                 * @instance
                 */
                CreateProject.prototype.sourceRef = "";

                /**
                 * Encodes the specified CreateProject message. Does not implicitly {@link evohime.desktop.v1.CreateProject.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.CreateProject
                 * @static
                 * @param {evohime.desktop.v1.CreateProject.$Properties} message CreateProject message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CreateProject.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.title != null && $Object.hasOwnProperty.call(message, "title") && message.title !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.title);
                    if (message.workspacePath != null && $Object.hasOwnProperty.call(message, "workspacePath") && message.workspacePath !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.workspacePath);
                    if (message.sourceRef != null && $Object.hasOwnProperty.call(message, "sourceRef") && message.sourceRef !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.sourceRef);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a CreateProject message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.CreateProject
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateProject & evohime.desktop.v1.CreateProject.$Shape} CreateProject
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CreateProject.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.CreateProject(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.title = value;
                                else
                                    delete message.title;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workspacePath = value;
                                else
                                    delete message.workspacePath;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.sourceRef = value;
                                else
                                    delete message.sourceRef;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for CreateProject
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.CreateProject
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                CreateProject.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.CreateProject";
                };

                return CreateProject;
            })();

            v1.CreateTask = (function() {

                /**
                 * Properties of a CreateTask.
                 * @typedef {Object} evohime.desktop.v1.CreateTask.$Properties
                 * @property {string|null} [taskId] CreateTask taskId
                 * @property {string|null} [projectId] CreateTask projectId
                 * @property {string|null} [parentId] CreateTask parentId
                 * @property {string|null} [title] CreateTask title
                 * @property {string|null} [description] CreateTask description
                 * @property {string|null} [sourceRef] CreateTask sourceRef
                 * @property {string|null} [acceptanceCriteria] CreateTask acceptanceCriteria
                 * @property {string|null} [nonGoals] CreateTask nonGoals
                 * @property {string|null} [status] CreateTask status
                 * @property {number|null} [priority] CreateTask priority
                 * @property {number|null} [estimate] CreateTask estimate
                 * @property {string|null} [complexity] CreateTask complexity
                 * @property {evohime.desktop.v1.TaskStatus|null} [statusCode] CreateTask statusCode
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a CreateTask.
                 * @memberof evohime.desktop.v1
                 * @interface ICreateTask
                 * @augments evohime.desktop.v1.CreateTask.$Properties
                 * @deprecated Use evohime.desktop.v1.CreateTask.$Properties instead.
                 */

                /**
                 * Shape of a CreateTask.
                 * @typedef {evohime.desktop.v1.CreateTask.$Properties} evohime.desktop.v1.CreateTask.$Shape
                 */

                /**
                 * Constructs a new CreateTask.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a CreateTask.
                 * @constructor
                 * @param {evohime.desktop.v1.CreateTask.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const CreateTask = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * CreateTask taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.taskId = "";

                /**
                 * CreateTask projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.projectId = "";

                /**
                 * CreateTask parentId.
                 * @member {string} parentId
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.parentId = "";

                /**
                 * CreateTask title.
                 * @member {string} title
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.title = "";

                /**
                 * CreateTask description.
                 * @member {string} description
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.description = "";

                /**
                 * CreateTask sourceRef.
                 * @member {string} sourceRef
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.sourceRef = "";

                /**
                 * CreateTask acceptanceCriteria.
                 * @member {string} acceptanceCriteria
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.acceptanceCriteria = "";

                /**
                 * CreateTask nonGoals.
                 * @member {string} nonGoals
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.nonGoals = "";

                /**
                 * CreateTask status.
                 * @member {string} status
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.status = "";

                /**
                 * CreateTask priority.
                 * @member {number} priority
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.priority = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * CreateTask estimate.
                 * @member {number} estimate
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.estimate = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * CreateTask complexity.
                 * @member {string} complexity
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.complexity = "";

                /**
                 * CreateTask statusCode.
                 * @member {evohime.desktop.v1.TaskStatus} statusCode
                 * @memberof evohime.desktop.v1.CreateTask
                 * @instance
                 */
                CreateTask.prototype.statusCode = 0;

                /**
                 * Encodes the specified CreateTask message. Does not implicitly {@link evohime.desktop.v1.CreateTask.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.CreateTask
                 * @static
                 * @param {evohime.desktop.v1.CreateTask.$Properties} message CreateTask message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CreateTask.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.projectId);
                    if (message.parentId != null && $Object.hasOwnProperty.call(message, "parentId") && message.parentId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.parentId);
                    if (message.title != null && $Object.hasOwnProperty.call(message, "title") && message.title !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.title);
                    if (message.description != null && $Object.hasOwnProperty.call(message, "description") && message.description !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.description);
                    if (message.sourceRef != null && $Object.hasOwnProperty.call(message, "sourceRef") && message.sourceRef !== "")
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.sourceRef);
                    if (message.acceptanceCriteria != null && $Object.hasOwnProperty.call(message, "acceptanceCriteria") && message.acceptanceCriteria !== "")
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.acceptanceCriteria);
                    if (message.nonGoals != null && $Object.hasOwnProperty.call(message, "nonGoals") && message.nonGoals !== "")
                        writer.uint32(/* id 8, wireType 2 =*/66).string(message.nonGoals);
                    if (message.status != null && $Object.hasOwnProperty.call(message, "status") && message.status !== "")
                        writer.uint32(/* id 9, wireType 2 =*/74).string(message.status);
                    if (message.priority != null && $Object.hasOwnProperty.call(message, "priority") && (typeof message.priority === "object" ? message.priority.low || message.priority.high : message.priority !== 0))
                        writer.uint32(/* id 10, wireType 0 =*/80).int64(message.priority);
                    if (message.estimate != null && $Object.hasOwnProperty.call(message, "estimate") && (typeof message.estimate === "object" ? message.estimate.low || message.estimate.high : message.estimate !== 0))
                        writer.uint32(/* id 11, wireType 0 =*/88).int64(message.estimate);
                    if (message.complexity != null && $Object.hasOwnProperty.call(message, "complexity") && message.complexity !== "")
                        writer.uint32(/* id 12, wireType 2 =*/98).string(message.complexity);
                    if (message.statusCode != null && $Object.hasOwnProperty.call(message, "statusCode") && message.statusCode !== 0)
                        writer.uint32(/* id 13, wireType 0 =*/104).int32(message.statusCode);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a CreateTask message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.CreateTask
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateTask & evohime.desktop.v1.CreateTask.$Shape} CreateTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CreateTask.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.CreateTask(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.parentId = value;
                                else
                                    delete message.parentId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.title = value;
                                else
                                    delete message.title;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.description = value;
                                else
                                    delete message.description;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.sourceRef = value;
                                else
                                    delete message.sourceRef;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.acceptanceCriteria = value;
                                else
                                    delete message.acceptanceCriteria;
                                continue;
                            }
                        case 8: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.nonGoals = value;
                                else
                                    delete message.nonGoals;
                                continue;
                            }
                        case 9: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.status = value;
                                else
                                    delete message.status;
                                continue;
                            }
                        case 10: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.int64()) === "object" ? value.low || value.high : value !== 0)
                                    message.priority = value;
                                else
                                    delete message.priority;
                                continue;
                            }
                        case 11: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.int64()) === "object" ? value.low || value.high : value !== 0)
                                    message.estimate = value;
                                else
                                    delete message.estimate;
                                continue;
                            }
                        case 12: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.complexity = value;
                                else
                                    delete message.complexity;
                                continue;
                            }
                        case 13: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.int32())
                                    message.statusCode = value;
                                else
                                    delete message.statusCode;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for CreateTask
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.CreateTask
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                CreateTask.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.CreateTask";
                };

                return CreateTask;
            })();

            v1.UpdateTaskStatus = (function() {

                /**
                 * Properties of an UpdateTaskStatus.
                 * @typedef {Object} evohime.desktop.v1.UpdateTaskStatus.$Properties
                 * @property {string|null} [taskId] UpdateTaskStatus taskId
                 * @property {number|null} [expectedVersion] UpdateTaskStatus expectedVersion
                 * @property {string|null} [status] UpdateTaskStatus status
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an UpdateTaskStatus.
                 * @memberof evohime.desktop.v1
                 * @interface IUpdateTaskStatus
                 * @augments evohime.desktop.v1.UpdateTaskStatus.$Properties
                 * @deprecated Use evohime.desktop.v1.UpdateTaskStatus.$Properties instead.
                 */

                /**
                 * Shape of an UpdateTaskStatus.
                 * @typedef {evohime.desktop.v1.UpdateTaskStatus.$Properties} evohime.desktop.v1.UpdateTaskStatus.$Shape
                 */

                /**
                 * Constructs a new UpdateTaskStatus.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an UpdateTaskStatus.
                 * @constructor
                 * @param {evohime.desktop.v1.UpdateTaskStatus.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const UpdateTaskStatus = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * UpdateTaskStatus taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.UpdateTaskStatus
                 * @instance
                 */
                UpdateTaskStatus.prototype.taskId = "";

                /**
                 * UpdateTaskStatus expectedVersion.
                 * @member {number} expectedVersion
                 * @memberof evohime.desktop.v1.UpdateTaskStatus
                 * @instance
                 */
                UpdateTaskStatus.prototype.expectedVersion = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * UpdateTaskStatus status.
                 * @member {string} status
                 * @memberof evohime.desktop.v1.UpdateTaskStatus
                 * @instance
                 */
                UpdateTaskStatus.prototype.status = "";

                /**
                 * Encodes the specified UpdateTaskStatus message. Does not implicitly {@link evohime.desktop.v1.UpdateTaskStatus.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.UpdateTaskStatus
                 * @static
                 * @param {evohime.desktop.v1.UpdateTaskStatus.$Properties} message UpdateTaskStatus message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                UpdateTaskStatus.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.expectedVersion != null && $Object.hasOwnProperty.call(message, "expectedVersion") && (typeof message.expectedVersion === "object" ? message.expectedVersion.low || message.expectedVersion.high : message.expectedVersion !== 0))
                        writer.uint32(/* id 2, wireType 0 =*/16).int64(message.expectedVersion);
                    if (message.status != null && $Object.hasOwnProperty.call(message, "status") && message.status !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.status);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an UpdateTaskStatus message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.UpdateTaskStatus
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.UpdateTaskStatus & evohime.desktop.v1.UpdateTaskStatus.$Shape} UpdateTaskStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                UpdateTaskStatus.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.UpdateTaskStatus(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.int64()) === "object" ? value.low || value.high : value !== 0)
                                    message.expectedVersion = value;
                                else
                                    delete message.expectedVersion;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.status = value;
                                else
                                    delete message.status;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for UpdateTaskStatus
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.UpdateTaskStatus
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                UpdateTaskStatus.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.UpdateTaskStatus";
                };

                return UpdateTaskStatus;
            })();

            v1.AddTaskEdge = (function() {

                /**
                 * Properties of an AddTaskEdge.
                 * @typedef {Object} evohime.desktop.v1.AddTaskEdge.$Properties
                 * @property {string|null} [fromTaskId] AddTaskEdge fromTaskId
                 * @property {string|null} [toTaskId] AddTaskEdge toTaskId
                 * @property {string|null} [kind] AddTaskEdge kind
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an AddTaskEdge.
                 * @memberof evohime.desktop.v1
                 * @interface IAddTaskEdge
                 * @augments evohime.desktop.v1.AddTaskEdge.$Properties
                 * @deprecated Use evohime.desktop.v1.AddTaskEdge.$Properties instead.
                 */

                /**
                 * Shape of an AddTaskEdge.
                 * @typedef {evohime.desktop.v1.AddTaskEdge.$Properties} evohime.desktop.v1.AddTaskEdge.$Shape
                 */

                /**
                 * Constructs a new AddTaskEdge.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an AddTaskEdge.
                 * @constructor
                 * @param {evohime.desktop.v1.AddTaskEdge.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const AddTaskEdge = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * AddTaskEdge fromTaskId.
                 * @member {string} fromTaskId
                 * @memberof evohime.desktop.v1.AddTaskEdge
                 * @instance
                 */
                AddTaskEdge.prototype.fromTaskId = "";

                /**
                 * AddTaskEdge toTaskId.
                 * @member {string} toTaskId
                 * @memberof evohime.desktop.v1.AddTaskEdge
                 * @instance
                 */
                AddTaskEdge.prototype.toTaskId = "";

                /**
                 * AddTaskEdge kind.
                 * @member {string} kind
                 * @memberof evohime.desktop.v1.AddTaskEdge
                 * @instance
                 */
                AddTaskEdge.prototype.kind = "";

                /**
                 * Encodes the specified AddTaskEdge message. Does not implicitly {@link evohime.desktop.v1.AddTaskEdge.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.AddTaskEdge
                 * @static
                 * @param {evohime.desktop.v1.AddTaskEdge.$Properties} message AddTaskEdge message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                AddTaskEdge.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.fromTaskId != null && $Object.hasOwnProperty.call(message, "fromTaskId") && message.fromTaskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.fromTaskId);
                    if (message.toTaskId != null && $Object.hasOwnProperty.call(message, "toTaskId") && message.toTaskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.toTaskId);
                    if (message.kind != null && $Object.hasOwnProperty.call(message, "kind") && message.kind !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.kind);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an AddTaskEdge message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.AddTaskEdge
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AddTaskEdge & evohime.desktop.v1.AddTaskEdge.$Shape} AddTaskEdge
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                AddTaskEdge.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.AddTaskEdge(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.fromTaskId = value;
                                else
                                    delete message.fromTaskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.toTaskId = value;
                                else
                                    delete message.toTaskId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.kind = value;
                                else
                                    delete message.kind;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for AddTaskEdge
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.AddTaskEdge
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                AddTaskEdge.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.AddTaskEdge";
                };

                return AddTaskEdge;
            })();

            v1.GetTaskGraph = (function() {

                /**
                 * Properties of a GetTaskGraph.
                 * @typedef {Object} evohime.desktop.v1.GetTaskGraph.$Properties
                 * @property {string|null} [projectId] GetTaskGraph projectId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GetTaskGraph.
                 * @memberof evohime.desktop.v1
                 * @interface IGetTaskGraph
                 * @augments evohime.desktop.v1.GetTaskGraph.$Properties
                 * @deprecated Use evohime.desktop.v1.GetTaskGraph.$Properties instead.
                 */

                /**
                 * Shape of a GetTaskGraph.
                 * @typedef {evohime.desktop.v1.GetTaskGraph.$Properties} evohime.desktop.v1.GetTaskGraph.$Shape
                 */

                /**
                 * Constructs a new GetTaskGraph.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GetTaskGraph.
                 * @constructor
                 * @param {evohime.desktop.v1.GetTaskGraph.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GetTaskGraph = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GetTaskGraph projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.GetTaskGraph
                 * @instance
                 */
                GetTaskGraph.prototype.projectId = "";

                /**
                 * Encodes the specified GetTaskGraph message. Does not implicitly {@link evohime.desktop.v1.GetTaskGraph.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GetTaskGraph
                 * @static
                 * @param {evohime.desktop.v1.GetTaskGraph.$Properties} message GetTaskGraph message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GetTaskGraph.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GetTaskGraph message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GetTaskGraph
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskGraph & evohime.desktop.v1.GetTaskGraph.$Shape} GetTaskGraph
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GetTaskGraph.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GetTaskGraph(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GetTaskGraph
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GetTaskGraph
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GetTaskGraph.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GetTaskGraph";
                };

                return GetTaskGraph;
            })();

            v1.NextReadyTask = (function() {

                /**
                 * Properties of a NextReadyTask.
                 * @typedef {Object} evohime.desktop.v1.NextReadyTask.$Properties
                 * @property {string|null} [projectId] NextReadyTask projectId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a NextReadyTask.
                 * @memberof evohime.desktop.v1
                 * @interface INextReadyTask
                 * @augments evohime.desktop.v1.NextReadyTask.$Properties
                 * @deprecated Use evohime.desktop.v1.NextReadyTask.$Properties instead.
                 */

                /**
                 * Shape of a NextReadyTask.
                 * @typedef {evohime.desktop.v1.NextReadyTask.$Properties} evohime.desktop.v1.NextReadyTask.$Shape
                 */

                /**
                 * Constructs a new NextReadyTask.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a NextReadyTask.
                 * @constructor
                 * @param {evohime.desktop.v1.NextReadyTask.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const NextReadyTask = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * NextReadyTask projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.NextReadyTask
                 * @instance
                 */
                NextReadyTask.prototype.projectId = "";

                /**
                 * Encodes the specified NextReadyTask message. Does not implicitly {@link evohime.desktop.v1.NextReadyTask.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.NextReadyTask
                 * @static
                 * @param {evohime.desktop.v1.NextReadyTask.$Properties} message NextReadyTask message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NextReadyTask.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a NextReadyTask message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.NextReadyTask
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.NextReadyTask & evohime.desktop.v1.NextReadyTask.$Shape} NextReadyTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NextReadyTask.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.NextReadyTask(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for NextReadyTask
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.NextReadyTask
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                NextReadyTask.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.NextReadyTask";
                };

                return NextReadyTask;
            })();

            v1.ImportPrd = (function() {

                /**
                 * Properties of an ImportPrd.
                 * @typedef {Object} evohime.desktop.v1.ImportPrd.$Properties
                 * @property {string|null} [importId] ImportPrd importId
                 * @property {string|null} [projectId] ImportPrd projectId
                 * @property {string|null} [origin] ImportPrd origin
                 * @property {string|null} [version] ImportPrd version
                 * @property {string|null} [sourceText] ImportPrd sourceText
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an ImportPrd.
                 * @memberof evohime.desktop.v1
                 * @interface IImportPrd
                 * @augments evohime.desktop.v1.ImportPrd.$Properties
                 * @deprecated Use evohime.desktop.v1.ImportPrd.$Properties instead.
                 */

                /**
                 * Shape of an ImportPrd.
                 * @typedef {evohime.desktop.v1.ImportPrd.$Properties} evohime.desktop.v1.ImportPrd.$Shape
                 */

                /**
                 * Constructs a new ImportPrd.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an ImportPrd.
                 * @constructor
                 * @param {evohime.desktop.v1.ImportPrd.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ImportPrd = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ImportPrd importId.
                 * @member {string} importId
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @instance
                 */
                ImportPrd.prototype.importId = "";

                /**
                 * ImportPrd projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @instance
                 */
                ImportPrd.prototype.projectId = "";

                /**
                 * ImportPrd origin.
                 * @member {string} origin
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @instance
                 */
                ImportPrd.prototype.origin = "";

                /**
                 * ImportPrd version.
                 * @member {string} version
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @instance
                 */
                ImportPrd.prototype.version = "";

                /**
                 * ImportPrd sourceText.
                 * @member {string} sourceText
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @instance
                 */
                ImportPrd.prototype.sourceText = "";

                /**
                 * Encodes the specified ImportPrd message. Does not implicitly {@link evohime.desktop.v1.ImportPrd.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @static
                 * @param {evohime.desktop.v1.ImportPrd.$Properties} message ImportPrd message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ImportPrd.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.importId != null && $Object.hasOwnProperty.call(message, "importId") && message.importId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.importId);
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.projectId);
                    if (message.origin != null && $Object.hasOwnProperty.call(message, "origin") && message.origin !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.origin);
                    if (message.version != null && $Object.hasOwnProperty.call(message, "version") && message.version !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.version);
                    if (message.sourceText != null && $Object.hasOwnProperty.call(message, "sourceText") && message.sourceText !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.sourceText);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an ImportPrd message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ImportPrd & evohime.desktop.v1.ImportPrd.$Shape} ImportPrd
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ImportPrd.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ImportPrd(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.importId = value;
                                else
                                    delete message.importId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.origin = value;
                                else
                                    delete message.origin;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.version = value;
                                else
                                    delete message.version;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.sourceText = value;
                                else
                                    delete message.sourceText;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ImportPrd
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ImportPrd
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ImportPrd.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ImportPrd";
                };

                return ImportPrd;
            })();

            v1.GetTaskHistory = (function() {

                /**
                 * Properties of a GetTaskHistory.
                 * @typedef {Object} evohime.desktop.v1.GetTaskHistory.$Properties
                 * @property {string|null} [taskId] GetTaskHistory taskId
                 * @property {number|null} [limit] GetTaskHistory limit
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GetTaskHistory.
                 * @memberof evohime.desktop.v1
                 * @interface IGetTaskHistory
                 * @augments evohime.desktop.v1.GetTaskHistory.$Properties
                 * @deprecated Use evohime.desktop.v1.GetTaskHistory.$Properties instead.
                 */

                /**
                 * Shape of a GetTaskHistory.
                 * @typedef {evohime.desktop.v1.GetTaskHistory.$Properties} evohime.desktop.v1.GetTaskHistory.$Shape
                 */

                /**
                 * Constructs a new GetTaskHistory.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GetTaskHistory.
                 * @constructor
                 * @param {evohime.desktop.v1.GetTaskHistory.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GetTaskHistory = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GetTaskHistory taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.GetTaskHistory
                 * @instance
                 */
                GetTaskHistory.prototype.taskId = "";

                /**
                 * GetTaskHistory limit.
                 * @member {number} limit
                 * @memberof evohime.desktop.v1.GetTaskHistory
                 * @instance
                 */
                GetTaskHistory.prototype.limit = 0;

                /**
                 * Encodes the specified GetTaskHistory message. Does not implicitly {@link evohime.desktop.v1.GetTaskHistory.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GetTaskHistory
                 * @static
                 * @param {evohime.desktop.v1.GetTaskHistory.$Properties} message GetTaskHistory message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GetTaskHistory.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.limit != null && $Object.hasOwnProperty.call(message, "limit") && message.limit !== 0)
                        writer.uint32(/* id 2, wireType 0 =*/16).uint32(message.limit);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GetTaskHistory message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GetTaskHistory
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskHistory & evohime.desktop.v1.GetTaskHistory.$Shape} GetTaskHistory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GetTaskHistory.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GetTaskHistory(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.limit = value;
                                else
                                    delete message.limit;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GetTaskHistory
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GetTaskHistory
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GetTaskHistory.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GetTaskHistory";
                };

                return GetTaskHistory;
            })();

            v1.GetTaskContext = (function() {

                /**
                 * Properties of a GetTaskContext.
                 * @typedef {Object} evohime.desktop.v1.GetTaskContext.$Properties
                 * @property {string|null} [projectId] GetTaskContext projectId
                 * @property {string|null} [taskId] GetTaskContext taskId
                 * @property {number|null} [maxChars] GetTaskContext maxChars
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GetTaskContext.
                 * @memberof evohime.desktop.v1
                 * @interface IGetTaskContext
                 * @augments evohime.desktop.v1.GetTaskContext.$Properties
                 * @deprecated Use evohime.desktop.v1.GetTaskContext.$Properties instead.
                 */

                /**
                 * Shape of a GetTaskContext.
                 * @typedef {evohime.desktop.v1.GetTaskContext.$Properties} evohime.desktop.v1.GetTaskContext.$Shape
                 */

                /**
                 * Constructs a new GetTaskContext.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GetTaskContext.
                 * @constructor
                 * @param {evohime.desktop.v1.GetTaskContext.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GetTaskContext = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GetTaskContext projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.GetTaskContext
                 * @instance
                 */
                GetTaskContext.prototype.projectId = "";

                /**
                 * GetTaskContext taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.GetTaskContext
                 * @instance
                 */
                GetTaskContext.prototype.taskId = "";

                /**
                 * GetTaskContext maxChars.
                 * @member {number} maxChars
                 * @memberof evohime.desktop.v1.GetTaskContext
                 * @instance
                 */
                GetTaskContext.prototype.maxChars = 0;

                /**
                 * Encodes the specified GetTaskContext message. Does not implicitly {@link evohime.desktop.v1.GetTaskContext.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GetTaskContext
                 * @static
                 * @param {evohime.desktop.v1.GetTaskContext.$Properties} message GetTaskContext message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GetTaskContext.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.taskId);
                    if (message.maxChars != null && $Object.hasOwnProperty.call(message, "maxChars") && message.maxChars !== 0)
                        writer.uint32(/* id 3, wireType 0 =*/24).uint32(message.maxChars);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GetTaskContext message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GetTaskContext
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskContext & evohime.desktop.v1.GetTaskContext.$Shape} GetTaskContext
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GetTaskContext.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GetTaskContext(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxChars = value;
                                else
                                    delete message.maxChars;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GetTaskContext
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GetTaskContext
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GetTaskContext.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GetTaskContext";
                };

                return GetTaskContext;
            })();

            v1.GetTaskPlanSpec = (function() {

                /**
                 * Properties of a GetTaskPlanSpec.
                 * @typedef {Object} evohime.desktop.v1.GetTaskPlanSpec.$Properties
                 * @property {string|null} [projectId] GetTaskPlanSpec projectId
                 * @property {string|null} [taskId] GetTaskPlanSpec taskId
                 * @property {number|null} [maxChars] GetTaskPlanSpec maxChars
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GetTaskPlanSpec.
                 * @memberof evohime.desktop.v1
                 * @interface IGetTaskPlanSpec
                 * @augments evohime.desktop.v1.GetTaskPlanSpec.$Properties
                 * @deprecated Use evohime.desktop.v1.GetTaskPlanSpec.$Properties instead.
                 */

                /**
                 * Shape of a GetTaskPlanSpec.
                 * @typedef {evohime.desktop.v1.GetTaskPlanSpec.$Properties} evohime.desktop.v1.GetTaskPlanSpec.$Shape
                 */

                /**
                 * Constructs a new GetTaskPlanSpec.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GetTaskPlanSpec.
                 * @constructor
                 * @param {evohime.desktop.v1.GetTaskPlanSpec.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GetTaskPlanSpec = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GetTaskPlanSpec projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.GetTaskPlanSpec
                 * @instance
                 */
                GetTaskPlanSpec.prototype.projectId = "";

                /**
                 * GetTaskPlanSpec taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.GetTaskPlanSpec
                 * @instance
                 */
                GetTaskPlanSpec.prototype.taskId = "";

                /**
                 * GetTaskPlanSpec maxChars.
                 * @member {number} maxChars
                 * @memberof evohime.desktop.v1.GetTaskPlanSpec
                 * @instance
                 */
                GetTaskPlanSpec.prototype.maxChars = 0;

                /**
                 * Encodes the specified GetTaskPlanSpec message. Does not implicitly {@link evohime.desktop.v1.GetTaskPlanSpec.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GetTaskPlanSpec
                 * @static
                 * @param {evohime.desktop.v1.GetTaskPlanSpec.$Properties} message GetTaskPlanSpec message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GetTaskPlanSpec.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.taskId);
                    if (message.maxChars != null && $Object.hasOwnProperty.call(message, "maxChars") && message.maxChars !== 0)
                        writer.uint32(/* id 3, wireType 0 =*/24).uint32(message.maxChars);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GetTaskPlanSpec message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GetTaskPlanSpec
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskPlanSpec & evohime.desktop.v1.GetTaskPlanSpec.$Shape} GetTaskPlanSpec
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GetTaskPlanSpec.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GetTaskPlanSpec(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxChars = value;
                                else
                                    delete message.maxChars;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GetTaskPlanSpec
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GetTaskPlanSpec
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GetTaskPlanSpec.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GetTaskPlanSpec";
                };

                return GetTaskPlanSpec;
            })();

            v1.ApplyApprovedBuild = (function() {

                /**
                 * Properties of an ApplyApprovedBuild.
                 * @typedef {Object} evohime.desktop.v1.ApplyApprovedBuild.$Properties
                 * @property {string|null} [projectId] ApplyApprovedBuild projectId
                 * @property {Uint8Array|null} [approvedBuildJson] ApplyApprovedBuild approvedBuildJson
                 * @property {string|null} [runId] ApplyApprovedBuild runId
                 * @property {string|null} [taskId] ApplyApprovedBuild taskId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an ApplyApprovedBuild.
                 * @memberof evohime.desktop.v1
                 * @interface IApplyApprovedBuild
                 * @augments evohime.desktop.v1.ApplyApprovedBuild.$Properties
                 * @deprecated Use evohime.desktop.v1.ApplyApprovedBuild.$Properties instead.
                 */

                /**
                 * Shape of an ApplyApprovedBuild.
                 * @typedef {evohime.desktop.v1.ApplyApprovedBuild.$Properties} evohime.desktop.v1.ApplyApprovedBuild.$Shape
                 */

                /**
                 * Constructs a new ApplyApprovedBuild.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an ApplyApprovedBuild.
                 * @constructor
                 * @param {evohime.desktop.v1.ApplyApprovedBuild.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ApplyApprovedBuild = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ApplyApprovedBuild projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.ApplyApprovedBuild
                 * @instance
                 */
                ApplyApprovedBuild.prototype.projectId = "";

                /**
                 * ApplyApprovedBuild approvedBuildJson.
                 * @member {Uint8Array} approvedBuildJson
                 * @memberof evohime.desktop.v1.ApplyApprovedBuild
                 * @instance
                 */
                ApplyApprovedBuild.prototype.approvedBuildJson = $util.newBuffer([]);

                /**
                 * ApplyApprovedBuild runId.
                 * @member {string} runId
                 * @memberof evohime.desktop.v1.ApplyApprovedBuild
                 * @instance
                 */
                ApplyApprovedBuild.prototype.runId = "";

                /**
                 * ApplyApprovedBuild taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.ApplyApprovedBuild
                 * @instance
                 */
                ApplyApprovedBuild.prototype.taskId = "";

                /**
                 * Encodes the specified ApplyApprovedBuild message. Does not implicitly {@link evohime.desktop.v1.ApplyApprovedBuild.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ApplyApprovedBuild
                 * @static
                 * @param {evohime.desktop.v1.ApplyApprovedBuild.$Properties} message ApplyApprovedBuild message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ApplyApprovedBuild.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.approvedBuildJson != null && $Object.hasOwnProperty.call(message, "approvedBuildJson") && message.approvedBuildJson.length)
                        writer.uint32(/* id 2, wireType 2 =*/18).bytes(message.approvedBuildJson);
                    if (message.runId != null && $Object.hasOwnProperty.call(message, "runId") && message.runId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.runId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.taskId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an ApplyApprovedBuild message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ApplyApprovedBuild
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ApplyApprovedBuild & evohime.desktop.v1.ApplyApprovedBuild.$Shape} ApplyApprovedBuild
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ApplyApprovedBuild.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ApplyApprovedBuild(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.bytes()).length)
                                    message.approvedBuildJson = value;
                                else
                                    delete message.approvedBuildJson;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.runId = value;
                                else
                                    delete message.runId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ApplyApprovedBuild
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ApplyApprovedBuild
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ApplyApprovedBuild.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ApplyApprovedBuild";
                };

                return ApplyApprovedBuild;
            })();

            v1.PrepareBuild = (function() {

                /**
                 * Properties of a PrepareBuild.
                 * @typedef {Object} evohime.desktop.v1.PrepareBuild.$Properties
                 * @property {string|null} [projectId] PrepareBuild projectId
                 * @property {Uint8Array|null} [proposalJson] PrepareBuild proposalJson
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a PrepareBuild.
                 * @memberof evohime.desktop.v1
                 * @interface IPrepareBuild
                 * @augments evohime.desktop.v1.PrepareBuild.$Properties
                 * @deprecated Use evohime.desktop.v1.PrepareBuild.$Properties instead.
                 */

                /**
                 * Shape of a PrepareBuild.
                 * @typedef {evohime.desktop.v1.PrepareBuild.$Properties} evohime.desktop.v1.PrepareBuild.$Shape
                 */

                /**
                 * Constructs a new PrepareBuild.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a PrepareBuild.
                 * @constructor
                 * @param {evohime.desktop.v1.PrepareBuild.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const PrepareBuild = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * PrepareBuild projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.PrepareBuild
                 * @instance
                 */
                PrepareBuild.prototype.projectId = "";

                /**
                 * PrepareBuild proposalJson.
                 * @member {Uint8Array} proposalJson
                 * @memberof evohime.desktop.v1.PrepareBuild
                 * @instance
                 */
                PrepareBuild.prototype.proposalJson = $util.newBuffer([]);

                /**
                 * Encodes the specified PrepareBuild message. Does not implicitly {@link evohime.desktop.v1.PrepareBuild.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.PrepareBuild
                 * @static
                 * @param {evohime.desktop.v1.PrepareBuild.$Properties} message PrepareBuild message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                PrepareBuild.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.proposalJson != null && $Object.hasOwnProperty.call(message, "proposalJson") && message.proposalJson.length)
                        writer.uint32(/* id 2, wireType 2 =*/18).bytes(message.proposalJson);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a PrepareBuild message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.PrepareBuild
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PrepareBuild & evohime.desktop.v1.PrepareBuild.$Shape} PrepareBuild
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                PrepareBuild.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.PrepareBuild(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.bytes()).length)
                                    message.proposalJson = value;
                                else
                                    delete message.proposalJson;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for PrepareBuild
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.PrepareBuild
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                PrepareBuild.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.PrepareBuild";
                };

                return PrepareBuild;
            })();

            v1.GetTaskSnapshot = (function() {

                /**
                 * Properties of a GetTaskSnapshot.
                 * @typedef {Object} evohime.desktop.v1.GetTaskSnapshot.$Properties
                 * @property {string|null} [projectId] GetTaskSnapshot projectId
                 * @property {string|null} [taskId] GetTaskSnapshot taskId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GetTaskSnapshot.
                 * @memberof evohime.desktop.v1
                 * @interface IGetTaskSnapshot
                 * @augments evohime.desktop.v1.GetTaskSnapshot.$Properties
                 * @deprecated Use evohime.desktop.v1.GetTaskSnapshot.$Properties instead.
                 */

                /**
                 * Shape of a GetTaskSnapshot.
                 * @typedef {evohime.desktop.v1.GetTaskSnapshot.$Properties} evohime.desktop.v1.GetTaskSnapshot.$Shape
                 */

                /**
                 * Constructs a new GetTaskSnapshot.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GetTaskSnapshot.
                 * @constructor
                 * @param {evohime.desktop.v1.GetTaskSnapshot.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GetTaskSnapshot = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GetTaskSnapshot projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.GetTaskSnapshot
                 * @instance
                 */
                GetTaskSnapshot.prototype.projectId = "";

                /**
                 * GetTaskSnapshot taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.GetTaskSnapshot
                 * @instance
                 */
                GetTaskSnapshot.prototype.taskId = "";

                /**
                 * Encodes the specified GetTaskSnapshot message. Does not implicitly {@link evohime.desktop.v1.GetTaskSnapshot.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GetTaskSnapshot
                 * @static
                 * @param {evohime.desktop.v1.GetTaskSnapshot.$Properties} message GetTaskSnapshot message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GetTaskSnapshot.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.taskId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GetTaskSnapshot message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GetTaskSnapshot
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskSnapshot & evohime.desktop.v1.GetTaskSnapshot.$Shape} GetTaskSnapshot
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GetTaskSnapshot.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GetTaskSnapshot(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GetTaskSnapshot
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GetTaskSnapshot
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GetTaskSnapshot.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GetTaskSnapshot";
                };

                return GetTaskSnapshot;
            })();

            v1.RestoreTaskSnapshot = (function() {

                /**
                 * Properties of a RestoreTaskSnapshot.
                 * @typedef {Object} evohime.desktop.v1.RestoreTaskSnapshot.$Properties
                 * @property {string|null} [projectId] RestoreTaskSnapshot projectId
                 * @property {string|null} [taskId] RestoreTaskSnapshot taskId
                 * @property {string|null} [snapshotId] RestoreTaskSnapshot snapshotId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a RestoreTaskSnapshot.
                 * @memberof evohime.desktop.v1
                 * @interface IRestoreTaskSnapshot
                 * @augments evohime.desktop.v1.RestoreTaskSnapshot.$Properties
                 * @deprecated Use evohime.desktop.v1.RestoreTaskSnapshot.$Properties instead.
                 */

                /**
                 * Shape of a RestoreTaskSnapshot.
                 * @typedef {evohime.desktop.v1.RestoreTaskSnapshot.$Properties} evohime.desktop.v1.RestoreTaskSnapshot.$Shape
                 */

                /**
                 * Constructs a new RestoreTaskSnapshot.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a RestoreTaskSnapshot.
                 * @constructor
                 * @param {evohime.desktop.v1.RestoreTaskSnapshot.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const RestoreTaskSnapshot = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * RestoreTaskSnapshot projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.RestoreTaskSnapshot
                 * @instance
                 */
                RestoreTaskSnapshot.prototype.projectId = "";

                /**
                 * RestoreTaskSnapshot taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.RestoreTaskSnapshot
                 * @instance
                 */
                RestoreTaskSnapshot.prototype.taskId = "";

                /**
                 * RestoreTaskSnapshot snapshotId.
                 * @member {string} snapshotId
                 * @memberof evohime.desktop.v1.RestoreTaskSnapshot
                 * @instance
                 */
                RestoreTaskSnapshot.prototype.snapshotId = "";

                /**
                 * Encodes the specified RestoreTaskSnapshot message. Does not implicitly {@link evohime.desktop.v1.RestoreTaskSnapshot.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.RestoreTaskSnapshot
                 * @static
                 * @param {evohime.desktop.v1.RestoreTaskSnapshot.$Properties} message RestoreTaskSnapshot message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RestoreTaskSnapshot.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.taskId);
                    if (message.snapshotId != null && $Object.hasOwnProperty.call(message, "snapshotId") && message.snapshotId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.snapshotId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a RestoreTaskSnapshot message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.RestoreTaskSnapshot
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RestoreTaskSnapshot & evohime.desktop.v1.RestoreTaskSnapshot.$Shape} RestoreTaskSnapshot
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RestoreTaskSnapshot.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.RestoreTaskSnapshot(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.snapshotId = value;
                                else
                                    delete message.snapshotId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for RestoreTaskSnapshot
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.RestoreTaskSnapshot
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                RestoreTaskSnapshot.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.RestoreTaskSnapshot";
                };

                return RestoreTaskSnapshot;
            })();

            v1.GetBuildPolicy = (function() {

                /**
                 * Properties of a GetBuildPolicy.
                 * @typedef {Object} evohime.desktop.v1.GetBuildPolicy.$Properties
                 * @property {string|null} [projectId] GetBuildPolicy projectId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GetBuildPolicy.
                 * @memberof evohime.desktop.v1
                 * @interface IGetBuildPolicy
                 * @augments evohime.desktop.v1.GetBuildPolicy.$Properties
                 * @deprecated Use evohime.desktop.v1.GetBuildPolicy.$Properties instead.
                 */

                /**
                 * Shape of a GetBuildPolicy.
                 * @typedef {evohime.desktop.v1.GetBuildPolicy.$Properties} evohime.desktop.v1.GetBuildPolicy.$Shape
                 */

                /**
                 * Constructs a new GetBuildPolicy.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GetBuildPolicy.
                 * @constructor
                 * @param {evohime.desktop.v1.GetBuildPolicy.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GetBuildPolicy = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GetBuildPolicy projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.GetBuildPolicy
                 * @instance
                 */
                GetBuildPolicy.prototype.projectId = "";

                /**
                 * Encodes the specified GetBuildPolicy message. Does not implicitly {@link evohime.desktop.v1.GetBuildPolicy.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GetBuildPolicy
                 * @static
                 * @param {evohime.desktop.v1.GetBuildPolicy.$Properties} message GetBuildPolicy message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GetBuildPolicy.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GetBuildPolicy message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GetBuildPolicy
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetBuildPolicy & evohime.desktop.v1.GetBuildPolicy.$Shape} GetBuildPolicy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GetBuildPolicy.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GetBuildPolicy(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GetBuildPolicy
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GetBuildPolicy
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GetBuildPolicy.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GetBuildPolicy";
                };

                return GetBuildPolicy;
            })();

            v1.SaveBuildPolicy = (function() {

                /**
                 * Properties of a SaveBuildPolicy.
                 * @typedef {Object} evohime.desktop.v1.SaveBuildPolicy.$Properties
                 * @property {string|null} [projectId] SaveBuildPolicy projectId
                 * @property {Uint8Array|null} [policyJson] SaveBuildPolicy policyJson
                 * @property {number|null} [expectedVersion] SaveBuildPolicy expectedVersion
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a SaveBuildPolicy.
                 * @memberof evohime.desktop.v1
                 * @interface ISaveBuildPolicy
                 * @augments evohime.desktop.v1.SaveBuildPolicy.$Properties
                 * @deprecated Use evohime.desktop.v1.SaveBuildPolicy.$Properties instead.
                 */

                /**
                 * Shape of a SaveBuildPolicy.
                 * @typedef {evohime.desktop.v1.SaveBuildPolicy.$Properties} evohime.desktop.v1.SaveBuildPolicy.$Shape
                 */

                /**
                 * Constructs a new SaveBuildPolicy.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a SaveBuildPolicy.
                 * @constructor
                 * @param {evohime.desktop.v1.SaveBuildPolicy.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const SaveBuildPolicy = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * SaveBuildPolicy projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.SaveBuildPolicy
                 * @instance
                 */
                SaveBuildPolicy.prototype.projectId = "";

                /**
                 * SaveBuildPolicy policyJson.
                 * @member {Uint8Array} policyJson
                 * @memberof evohime.desktop.v1.SaveBuildPolicy
                 * @instance
                 */
                SaveBuildPolicy.prototype.policyJson = $util.newBuffer([]);

                /**
                 * SaveBuildPolicy expectedVersion.
                 * @member {number} expectedVersion
                 * @memberof evohime.desktop.v1.SaveBuildPolicy
                 * @instance
                 */
                SaveBuildPolicy.prototype.expectedVersion = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * Encodes the specified SaveBuildPolicy message. Does not implicitly {@link evohime.desktop.v1.SaveBuildPolicy.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.SaveBuildPolicy
                 * @static
                 * @param {evohime.desktop.v1.SaveBuildPolicy.$Properties} message SaveBuildPolicy message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                SaveBuildPolicy.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.policyJson != null && $Object.hasOwnProperty.call(message, "policyJson") && message.policyJson.length)
                        writer.uint32(/* id 2, wireType 2 =*/18).bytes(message.policyJson);
                    if (message.expectedVersion != null && $Object.hasOwnProperty.call(message, "expectedVersion") && (typeof message.expectedVersion === "object" ? message.expectedVersion.low || message.expectedVersion.high : message.expectedVersion !== 0))
                        writer.uint32(/* id 3, wireType 0 =*/24).int64(message.expectedVersion);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a SaveBuildPolicy message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.SaveBuildPolicy
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SaveBuildPolicy & evohime.desktop.v1.SaveBuildPolicy.$Shape} SaveBuildPolicy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                SaveBuildPolicy.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.SaveBuildPolicy(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.bytes()).length)
                                    message.policyJson = value;
                                else
                                    delete message.policyJson;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.int64()) === "object" ? value.low || value.high : value !== 0)
                                    message.expectedVersion = value;
                                else
                                    delete message.expectedVersion;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for SaveBuildPolicy
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.SaveBuildPolicy
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                SaveBuildPolicy.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.SaveBuildPolicy";
                };

                return SaveBuildPolicy;
            })();

            v1.StartTask = (function() {

                /**
                 * Properties of a StartTask.
                 * @typedef {Object} evohime.desktop.v1.StartTask.$Properties
                 * @property {string|null} [taskId] StartTask taskId
                 * @property {string|null} [prompt] StartTask prompt
                 * @property {string|null} [workspacePath] StartTask workspacePath
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a StartTask.
                 * @memberof evohime.desktop.v1
                 * @interface IStartTask
                 * @augments evohime.desktop.v1.StartTask.$Properties
                 * @deprecated Use evohime.desktop.v1.StartTask.$Properties instead.
                 */

                /**
                 * Shape of a StartTask.
                 * @typedef {evohime.desktop.v1.StartTask.$Properties} evohime.desktop.v1.StartTask.$Shape
                 */

                /**
                 * Constructs a new StartTask.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a StartTask.
                 * @constructor
                 * @param {evohime.desktop.v1.StartTask.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const StartTask = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * StartTask taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.StartTask
                 * @instance
                 */
                StartTask.prototype.taskId = "";

                /**
                 * StartTask prompt.
                 * @member {string} prompt
                 * @memberof evohime.desktop.v1.StartTask
                 * @instance
                 */
                StartTask.prototype.prompt = "";

                /**
                 * StartTask workspacePath.
                 * @member {string} workspacePath
                 * @memberof evohime.desktop.v1.StartTask
                 * @instance
                 */
                StartTask.prototype.workspacePath = "";

                /**
                 * Encodes the specified StartTask message. Does not implicitly {@link evohime.desktop.v1.StartTask.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.StartTask
                 * @static
                 * @param {evohime.desktop.v1.StartTask.$Properties} message StartTask message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                StartTask.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.prompt != null && $Object.hasOwnProperty.call(message, "prompt") && message.prompt !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.prompt);
                    if (message.workspacePath != null && $Object.hasOwnProperty.call(message, "workspacePath") && message.workspacePath !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.workspacePath);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a StartTask message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.StartTask
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StartTask & evohime.desktop.v1.StartTask.$Shape} StartTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                StartTask.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.StartTask(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.prompt = value;
                                else
                                    delete message.prompt;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workspacePath = value;
                                else
                                    delete message.workspacePath;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for StartTask
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.StartTask
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                StartTask.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.StartTask";
                };

                return StartTask;
            })();

            v1.ListWorkspace = (function() {

                /**
                 * Properties of a ListWorkspace.
                 * @typedef {Object} evohime.desktop.v1.ListWorkspace.$Properties
                 * @property {string|null} [workspacePath] ListWorkspace workspacePath
                 * @property {string|null} [relativePath] ListWorkspace relativePath
                 * @property {number|null} [maxEntries] ListWorkspace maxEntries
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ListWorkspace.
                 * @memberof evohime.desktop.v1
                 * @interface IListWorkspace
                 * @augments evohime.desktop.v1.ListWorkspace.$Properties
                 * @deprecated Use evohime.desktop.v1.ListWorkspace.$Properties instead.
                 */

                /**
                 * Shape of a ListWorkspace.
                 * @typedef {evohime.desktop.v1.ListWorkspace.$Properties} evohime.desktop.v1.ListWorkspace.$Shape
                 */

                /**
                 * Constructs a new ListWorkspace.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ListWorkspace.
                 * @constructor
                 * @param {evohime.desktop.v1.ListWorkspace.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ListWorkspace = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ListWorkspace workspacePath.
                 * @member {string} workspacePath
                 * @memberof evohime.desktop.v1.ListWorkspace
                 * @instance
                 */
                ListWorkspace.prototype.workspacePath = "";

                /**
                 * ListWorkspace relativePath.
                 * @member {string} relativePath
                 * @memberof evohime.desktop.v1.ListWorkspace
                 * @instance
                 */
                ListWorkspace.prototype.relativePath = "";

                /**
                 * ListWorkspace maxEntries.
                 * @member {number} maxEntries
                 * @memberof evohime.desktop.v1.ListWorkspace
                 * @instance
                 */
                ListWorkspace.prototype.maxEntries = 0;

                /**
                 * Encodes the specified ListWorkspace message. Does not implicitly {@link evohime.desktop.v1.ListWorkspace.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ListWorkspace
                 * @static
                 * @param {evohime.desktop.v1.ListWorkspace.$Properties} message ListWorkspace message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ListWorkspace.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.workspacePath != null && $Object.hasOwnProperty.call(message, "workspacePath") && message.workspacePath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.workspacePath);
                    if (message.relativePath != null && $Object.hasOwnProperty.call(message, "relativePath") && message.relativePath !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.relativePath);
                    if (message.maxEntries != null && $Object.hasOwnProperty.call(message, "maxEntries") && message.maxEntries !== 0)
                        writer.uint32(/* id 3, wireType 0 =*/24).uint32(message.maxEntries);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ListWorkspace message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ListWorkspace
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListWorkspace & evohime.desktop.v1.ListWorkspace.$Shape} ListWorkspace
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ListWorkspace.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ListWorkspace(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workspacePath = value;
                                else
                                    delete message.workspacePath;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.relativePath = value;
                                else
                                    delete message.relativePath;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxEntries = value;
                                else
                                    delete message.maxEntries;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ListWorkspace
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ListWorkspace
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ListWorkspace.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ListWorkspace";
                };

                return ListWorkspace;
            })();

            v1.ReadWorkspaceFile = (function() {

                /**
                 * Properties of a ReadWorkspaceFile.
                 * @typedef {Object} evohime.desktop.v1.ReadWorkspaceFile.$Properties
                 * @property {string|null} [workspacePath] ReadWorkspaceFile workspacePath
                 * @property {string|null} [relativePath] ReadWorkspaceFile relativePath
                 * @property {number|null} [maxBytes] ReadWorkspaceFile maxBytes
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ReadWorkspaceFile.
                 * @memberof evohime.desktop.v1
                 * @interface IReadWorkspaceFile
                 * @augments evohime.desktop.v1.ReadWorkspaceFile.$Properties
                 * @deprecated Use evohime.desktop.v1.ReadWorkspaceFile.$Properties instead.
                 */

                /**
                 * Shape of a ReadWorkspaceFile.
                 * @typedef {evohime.desktop.v1.ReadWorkspaceFile.$Properties} evohime.desktop.v1.ReadWorkspaceFile.$Shape
                 */

                /**
                 * Constructs a new ReadWorkspaceFile.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ReadWorkspaceFile.
                 * @constructor
                 * @param {evohime.desktop.v1.ReadWorkspaceFile.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ReadWorkspaceFile = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ReadWorkspaceFile workspacePath.
                 * @member {string} workspacePath
                 * @memberof evohime.desktop.v1.ReadWorkspaceFile
                 * @instance
                 */
                ReadWorkspaceFile.prototype.workspacePath = "";

                /**
                 * ReadWorkspaceFile relativePath.
                 * @member {string} relativePath
                 * @memberof evohime.desktop.v1.ReadWorkspaceFile
                 * @instance
                 */
                ReadWorkspaceFile.prototype.relativePath = "";

                /**
                 * ReadWorkspaceFile maxBytes.
                 * @member {number} maxBytes
                 * @memberof evohime.desktop.v1.ReadWorkspaceFile
                 * @instance
                 */
                ReadWorkspaceFile.prototype.maxBytes = 0;

                /**
                 * Encodes the specified ReadWorkspaceFile message. Does not implicitly {@link evohime.desktop.v1.ReadWorkspaceFile.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ReadWorkspaceFile
                 * @static
                 * @param {evohime.desktop.v1.ReadWorkspaceFile.$Properties} message ReadWorkspaceFile message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ReadWorkspaceFile.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.workspacePath != null && $Object.hasOwnProperty.call(message, "workspacePath") && message.workspacePath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.workspacePath);
                    if (message.relativePath != null && $Object.hasOwnProperty.call(message, "relativePath") && message.relativePath !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.relativePath);
                    if (message.maxBytes != null && $Object.hasOwnProperty.call(message, "maxBytes") && message.maxBytes !== 0)
                        writer.uint32(/* id 3, wireType 0 =*/24).uint32(message.maxBytes);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ReadWorkspaceFile message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ReadWorkspaceFile
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReadWorkspaceFile & evohime.desktop.v1.ReadWorkspaceFile.$Shape} ReadWorkspaceFile
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ReadWorkspaceFile.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ReadWorkspaceFile(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workspacePath = value;
                                else
                                    delete message.workspacePath;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.relativePath = value;
                                else
                                    delete message.relativePath;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxBytes = value;
                                else
                                    delete message.maxBytes;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ReadWorkspaceFile
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ReadWorkspaceFile
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ReadWorkspaceFile.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ReadWorkspaceFile";
                };

                return ReadWorkspaceFile;
            })();

            v1.GitStatus = (function() {

                /**
                 * Properties of a GitStatus.
                 * @typedef {Object} evohime.desktop.v1.GitStatus.$Properties
                 * @property {string|null} [workspacePath] GitStatus workspacePath
                 * @property {number|null} [maxBytes] GitStatus maxBytes
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GitStatus.
                 * @memberof evohime.desktop.v1
                 * @interface IGitStatus
                 * @augments evohime.desktop.v1.GitStatus.$Properties
                 * @deprecated Use evohime.desktop.v1.GitStatus.$Properties instead.
                 */

                /**
                 * Shape of a GitStatus.
                 * @typedef {evohime.desktop.v1.GitStatus.$Properties} evohime.desktop.v1.GitStatus.$Shape
                 */

                /**
                 * Constructs a new GitStatus.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GitStatus.
                 * @constructor
                 * @param {evohime.desktop.v1.GitStatus.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GitStatus = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GitStatus workspacePath.
                 * @member {string} workspacePath
                 * @memberof evohime.desktop.v1.GitStatus
                 * @instance
                 */
                GitStatus.prototype.workspacePath = "";

                /**
                 * GitStatus maxBytes.
                 * @member {number} maxBytes
                 * @memberof evohime.desktop.v1.GitStatus
                 * @instance
                 */
                GitStatus.prototype.maxBytes = 0;

                /**
                 * Encodes the specified GitStatus message. Does not implicitly {@link evohime.desktop.v1.GitStatus.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GitStatus
                 * @static
                 * @param {evohime.desktop.v1.GitStatus.$Properties} message GitStatus message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GitStatus.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.workspacePath != null && $Object.hasOwnProperty.call(message, "workspacePath") && message.workspacePath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.workspacePath);
                    if (message.maxBytes != null && $Object.hasOwnProperty.call(message, "maxBytes") && message.maxBytes !== 0)
                        writer.uint32(/* id 2, wireType 0 =*/16).uint32(message.maxBytes);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GitStatus message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GitStatus
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GitStatus & evohime.desktop.v1.GitStatus.$Shape} GitStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GitStatus.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GitStatus(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workspacePath = value;
                                else
                                    delete message.workspacePath;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxBytes = value;
                                else
                                    delete message.maxBytes;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GitStatus
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GitStatus
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GitStatus.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GitStatus";
                };

                return GitStatus;
            })();

            v1.GitDiff = (function() {

                /**
                 * Properties of a GitDiff.
                 * @typedef {Object} evohime.desktop.v1.GitDiff.$Properties
                 * @property {string|null} [workspacePath] GitDiff workspacePath
                 * @property {string|null} [relativePath] GitDiff relativePath
                 * @property {number|null} [maxBytes] GitDiff maxBytes
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GitDiff.
                 * @memberof evohime.desktop.v1
                 * @interface IGitDiff
                 * @augments evohime.desktop.v1.GitDiff.$Properties
                 * @deprecated Use evohime.desktop.v1.GitDiff.$Properties instead.
                 */

                /**
                 * Shape of a GitDiff.
                 * @typedef {evohime.desktop.v1.GitDiff.$Properties} evohime.desktop.v1.GitDiff.$Shape
                 */

                /**
                 * Constructs a new GitDiff.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GitDiff.
                 * @constructor
                 * @param {evohime.desktop.v1.GitDiff.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GitDiff = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GitDiff workspacePath.
                 * @member {string} workspacePath
                 * @memberof evohime.desktop.v1.GitDiff
                 * @instance
                 */
                GitDiff.prototype.workspacePath = "";

                /**
                 * GitDiff relativePath.
                 * @member {string} relativePath
                 * @memberof evohime.desktop.v1.GitDiff
                 * @instance
                 */
                GitDiff.prototype.relativePath = "";

                /**
                 * GitDiff maxBytes.
                 * @member {number} maxBytes
                 * @memberof evohime.desktop.v1.GitDiff
                 * @instance
                 */
                GitDiff.prototype.maxBytes = 0;

                /**
                 * Encodes the specified GitDiff message. Does not implicitly {@link evohime.desktop.v1.GitDiff.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GitDiff
                 * @static
                 * @param {evohime.desktop.v1.GitDiff.$Properties} message GitDiff message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GitDiff.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.workspacePath != null && $Object.hasOwnProperty.call(message, "workspacePath") && message.workspacePath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.workspacePath);
                    if (message.relativePath != null && $Object.hasOwnProperty.call(message, "relativePath") && message.relativePath !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.relativePath);
                    if (message.maxBytes != null && $Object.hasOwnProperty.call(message, "maxBytes") && message.maxBytes !== 0)
                        writer.uint32(/* id 3, wireType 0 =*/24).uint32(message.maxBytes);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GitDiff message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GitDiff
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GitDiff & evohime.desktop.v1.GitDiff.$Shape} GitDiff
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GitDiff.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GitDiff(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workspacePath = value;
                                else
                                    delete message.workspacePath;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.relativePath = value;
                                else
                                    delete message.relativePath;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxBytes = value;
                                else
                                    delete message.maxBytes;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GitDiff
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GitDiff
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GitDiff.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GitDiff";
                };

                return GitDiff;
            })();

            v1.TerminalExecute = (function() {

                /**
                 * Properties of a TerminalExecute.
                 * @typedef {Object} evohime.desktop.v1.TerminalExecute.$Properties
                 * @property {string|null} [taskId] TerminalExecute taskId
                 * @property {string|null} [workspacePath] TerminalExecute workspacePath
                 * @property {string|null} [program] TerminalExecute program
                 * @property {Array.<string>|null} [args] TerminalExecute args
                 * @property {string|null} [cwd] TerminalExecute cwd
                 * @property {number|null} [timeoutMs] TerminalExecute timeoutMs
                 * @property {string|null} [approvalId] TerminalExecute approvalId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a TerminalExecute.
                 * @memberof evohime.desktop.v1
                 * @interface ITerminalExecute
                 * @augments evohime.desktop.v1.TerminalExecute.$Properties
                 * @deprecated Use evohime.desktop.v1.TerminalExecute.$Properties instead.
                 */

                /**
                 * Shape of a TerminalExecute.
                 * @typedef {evohime.desktop.v1.TerminalExecute.$Properties} evohime.desktop.v1.TerminalExecute.$Shape
                 */

                /**
                 * Constructs a new TerminalExecute.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a TerminalExecute.
                 * @constructor
                 * @param {evohime.desktop.v1.TerminalExecute.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const TerminalExecute = function (properties) {
                    this.args = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * TerminalExecute taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @instance
                 */
                TerminalExecute.prototype.taskId = "";

                /**
                 * TerminalExecute workspacePath.
                 * @member {string} workspacePath
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @instance
                 */
                TerminalExecute.prototype.workspacePath = "";

                /**
                 * TerminalExecute program.
                 * @member {string} program
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @instance
                 */
                TerminalExecute.prototype.program = "";

                /**
                 * TerminalExecute args.
                 * @member {Array.<string>} args
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @instance
                 */
                TerminalExecute.prototype.args = $util.emptyArray;

                /**
                 * TerminalExecute cwd.
                 * @member {string} cwd
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @instance
                 */
                TerminalExecute.prototype.cwd = "";

                /**
                 * TerminalExecute timeoutMs.
                 * @member {number} timeoutMs
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @instance
                 */
                TerminalExecute.prototype.timeoutMs = 0;

                /**
                 * TerminalExecute approvalId.
                 * @member {string} approvalId
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @instance
                 */
                TerminalExecute.prototype.approvalId = "";

                /**
                 * Encodes the specified TerminalExecute message. Does not implicitly {@link evohime.desktop.v1.TerminalExecute.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @static
                 * @param {evohime.desktop.v1.TerminalExecute.$Properties} message TerminalExecute message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                TerminalExecute.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.workspacePath != null && $Object.hasOwnProperty.call(message, "workspacePath") && message.workspacePath !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.workspacePath);
                    if (message.program != null && $Object.hasOwnProperty.call(message, "program") && message.program !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.program);
                    if (message.args != null && message.args.length)
                        for (let i = 0; i < message.args.length; ++i)
                            writer.uint32(/* id 4, wireType 2 =*/34).string(message.args[i]);
                    if (message.cwd != null && $Object.hasOwnProperty.call(message, "cwd") && message.cwd !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.cwd);
                    if (message.timeoutMs != null && $Object.hasOwnProperty.call(message, "timeoutMs") && message.timeoutMs !== 0)
                        writer.uint32(/* id 6, wireType 0 =*/48).uint32(message.timeoutMs);
                    if (message.approvalId != null && $Object.hasOwnProperty.call(message, "approvalId") && message.approvalId !== "")
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.approvalId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a TerminalExecute message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.TerminalExecute & evohime.desktop.v1.TerminalExecute.$Shape} TerminalExecute
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                TerminalExecute.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.TerminalExecute(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workspacePath = value;
                                else
                                    delete message.workspacePath;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.program = value;
                                else
                                    delete message.program;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.args && message.args.length))
                                    message.args = [];
                                message.args.push(reader.stringVerify());
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.cwd = value;
                                else
                                    delete message.cwd;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.timeoutMs = value;
                                else
                                    delete message.timeoutMs;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.approvalId = value;
                                else
                                    delete message.approvalId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for TerminalExecute
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.TerminalExecute
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                TerminalExecute.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.TerminalExecute";
                };

                return TerminalExecute;
            })();

            v1.StopTask = (function() {

                /**
                 * Properties of a StopTask.
                 * @typedef {Object} evohime.desktop.v1.StopTask.$Properties
                 * @property {string|null} [taskId] StopTask taskId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a StopTask.
                 * @memberof evohime.desktop.v1
                 * @interface IStopTask
                 * @augments evohime.desktop.v1.StopTask.$Properties
                 * @deprecated Use evohime.desktop.v1.StopTask.$Properties instead.
                 */

                /**
                 * Shape of a StopTask.
                 * @typedef {evohime.desktop.v1.StopTask.$Properties} evohime.desktop.v1.StopTask.$Shape
                 */

                /**
                 * Constructs a new StopTask.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a StopTask.
                 * @constructor
                 * @param {evohime.desktop.v1.StopTask.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const StopTask = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * StopTask taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.StopTask
                 * @instance
                 */
                StopTask.prototype.taskId = "";

                /**
                 * Encodes the specified StopTask message. Does not implicitly {@link evohime.desktop.v1.StopTask.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.StopTask
                 * @static
                 * @param {evohime.desktop.v1.StopTask.$Properties} message StopTask message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                StopTask.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a StopTask message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.StopTask
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StopTask & evohime.desktop.v1.StopTask.$Shape} StopTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                StopTask.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.StopTask(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for StopTask
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.StopTask
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                StopTask.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.StopTask";
                };

                return StopTask;
            })();

            v1.ResolveApproval = (function() {

                /**
                 * Properties of a ResolveApproval.
                 * @typedef {Object} evohime.desktop.v1.ResolveApproval.$Properties
                 * @property {string|null} [approvalId] ResolveApproval approvalId
                 * @property {boolean|null} [granted] ResolveApproval granted
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ResolveApproval.
                 * @memberof evohime.desktop.v1
                 * @interface IResolveApproval
                 * @augments evohime.desktop.v1.ResolveApproval.$Properties
                 * @deprecated Use evohime.desktop.v1.ResolveApproval.$Properties instead.
                 */

                /**
                 * Shape of a ResolveApproval.
                 * @typedef {evohime.desktop.v1.ResolveApproval.$Properties} evohime.desktop.v1.ResolveApproval.$Shape
                 */

                /**
                 * Constructs a new ResolveApproval.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ResolveApproval.
                 * @constructor
                 * @param {evohime.desktop.v1.ResolveApproval.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ResolveApproval = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ResolveApproval approvalId.
                 * @member {string} approvalId
                 * @memberof evohime.desktop.v1.ResolveApproval
                 * @instance
                 */
                ResolveApproval.prototype.approvalId = "";

                /**
                 * ResolveApproval granted.
                 * @member {boolean} granted
                 * @memberof evohime.desktop.v1.ResolveApproval
                 * @instance
                 */
                ResolveApproval.prototype.granted = false;

                /**
                 * Encodes the specified ResolveApproval message. Does not implicitly {@link evohime.desktop.v1.ResolveApproval.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ResolveApproval
                 * @static
                 * @param {evohime.desktop.v1.ResolveApproval.$Properties} message ResolveApproval message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ResolveApproval.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.approvalId != null && $Object.hasOwnProperty.call(message, "approvalId") && message.approvalId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.approvalId);
                    if (message.granted != null && $Object.hasOwnProperty.call(message, "granted") && message.granted !== false)
                        writer.uint32(/* id 2, wireType 0 =*/16).bool(message.granted);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ResolveApproval message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ResolveApproval
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ResolveApproval & evohime.desktop.v1.ResolveApproval.$Shape} ResolveApproval
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ResolveApproval.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ResolveApproval(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.approvalId = value;
                                else
                                    delete message.approvalId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.bool())
                                    message.granted = value;
                                else
                                    delete message.granted;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ResolveApproval
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ResolveApproval
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ResolveApproval.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ResolveApproval";
                };

                return ResolveApproval;
            })();

            v1.RunDoctor = (function() {

                /**
                 * Properties of a RunDoctor.
                 * @typedef {Object} evohime.desktop.v1.RunDoctor.$Properties
                 * @property {string|null} [projectId] RunDoctor projectId
                 * @property {number|null} [detailLevel] RunDoctor detailLevel
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a RunDoctor.
                 * @memberof evohime.desktop.v1
                 * @interface IRunDoctor
                 * @augments evohime.desktop.v1.RunDoctor.$Properties
                 * @deprecated Use evohime.desktop.v1.RunDoctor.$Properties instead.
                 */

                /**
                 * Shape of a RunDoctor.
                 * @typedef {evohime.desktop.v1.RunDoctor.$Properties} evohime.desktop.v1.RunDoctor.$Shape
                 */

                /**
                 * Constructs a new RunDoctor.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a RunDoctor.
                 * @constructor
                 * @param {evohime.desktop.v1.RunDoctor.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const RunDoctor = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * RunDoctor projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.RunDoctor
                 * @instance
                 */
                RunDoctor.prototype.projectId = "";

                /**
                 * RunDoctor detailLevel.
                 * @member {number} detailLevel
                 * @memberof evohime.desktop.v1.RunDoctor
                 * @instance
                 */
                RunDoctor.prototype.detailLevel = 0;

                /**
                 * Encodes the specified RunDoctor message. Does not implicitly {@link evohime.desktop.v1.RunDoctor.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.RunDoctor
                 * @static
                 * @param {evohime.desktop.v1.RunDoctor.$Properties} message RunDoctor message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RunDoctor.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.projectId);
                    if (message.detailLevel != null && $Object.hasOwnProperty.call(message, "detailLevel") && message.detailLevel !== 0)
                        writer.uint32(/* id 2, wireType 0 =*/16).int32(message.detailLevel);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a RunDoctor message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.RunDoctor
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RunDoctor & evohime.desktop.v1.RunDoctor.$Shape} RunDoctor
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RunDoctor.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.RunDoctor(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.int32())
                                    message.detailLevel = value;
                                else
                                    delete message.detailLevel;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for RunDoctor
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.RunDoctor
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                RunDoctor.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.RunDoctor";
                };

                return RunDoctor;
            })();

            v1.ExportDoctorLogs = (function() {

                /**
                 * Properties of an ExportDoctorLogs.
                 * @typedef {Object} evohime.desktop.v1.ExportDoctorLogs.$Properties
                 * @property {string|null} [destinationPath] ExportDoctorLogs destinationPath
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an ExportDoctorLogs.
                 * @memberof evohime.desktop.v1
                 * @interface IExportDoctorLogs
                 * @augments evohime.desktop.v1.ExportDoctorLogs.$Properties
                 * @deprecated Use evohime.desktop.v1.ExportDoctorLogs.$Properties instead.
                 */

                /**
                 * Shape of an ExportDoctorLogs.
                 * @typedef {evohime.desktop.v1.ExportDoctorLogs.$Properties} evohime.desktop.v1.ExportDoctorLogs.$Shape
                 */

                /**
                 * Constructs a new ExportDoctorLogs.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an ExportDoctorLogs.
                 * @constructor
                 * @param {evohime.desktop.v1.ExportDoctorLogs.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ExportDoctorLogs = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ExportDoctorLogs destinationPath.
                 * @member {string} destinationPath
                 * @memberof evohime.desktop.v1.ExportDoctorLogs
                 * @instance
                 */
                ExportDoctorLogs.prototype.destinationPath = "";

                /**
                 * Encodes the specified ExportDoctorLogs message. Does not implicitly {@link evohime.desktop.v1.ExportDoctorLogs.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ExportDoctorLogs
                 * @static
                 * @param {evohime.desktop.v1.ExportDoctorLogs.$Properties} message ExportDoctorLogs message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ExportDoctorLogs.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.destinationPath != null && $Object.hasOwnProperty.call(message, "destinationPath") && message.destinationPath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.destinationPath);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an ExportDoctorLogs message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ExportDoctorLogs
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ExportDoctorLogs & evohime.desktop.v1.ExportDoctorLogs.$Shape} ExportDoctorLogs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ExportDoctorLogs.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ExportDoctorLogs(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.destinationPath = value;
                                else
                                    delete message.destinationPath;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ExportDoctorLogs
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ExportDoctorLogs
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ExportDoctorLogs.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ExportDoctorLogs";
                };

                return ExportDoctorLogs;
            })();

            v1.CreateDatabaseBackup = (function() {

                /**
                 * Properties of a CreateDatabaseBackup.
                 * @typedef {Object} evohime.desktop.v1.CreateDatabaseBackup.$Properties
                 * @property {string|null} [destinationPath] CreateDatabaseBackup destinationPath
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a CreateDatabaseBackup.
                 * @memberof evohime.desktop.v1
                 * @interface ICreateDatabaseBackup
                 * @augments evohime.desktop.v1.CreateDatabaseBackup.$Properties
                 * @deprecated Use evohime.desktop.v1.CreateDatabaseBackup.$Properties instead.
                 */

                /**
                 * Shape of a CreateDatabaseBackup.
                 * @typedef {evohime.desktop.v1.CreateDatabaseBackup.$Properties} evohime.desktop.v1.CreateDatabaseBackup.$Shape
                 */

                /**
                 * Constructs a new CreateDatabaseBackup.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a CreateDatabaseBackup.
                 * @constructor
                 * @param {evohime.desktop.v1.CreateDatabaseBackup.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const CreateDatabaseBackup = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * CreateDatabaseBackup destinationPath.
                 * @member {string} destinationPath
                 * @memberof evohime.desktop.v1.CreateDatabaseBackup
                 * @instance
                 */
                CreateDatabaseBackup.prototype.destinationPath = "";

                /**
                 * Encodes the specified CreateDatabaseBackup message. Does not implicitly {@link evohime.desktop.v1.CreateDatabaseBackup.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.CreateDatabaseBackup
                 * @static
                 * @param {evohime.desktop.v1.CreateDatabaseBackup.$Properties} message CreateDatabaseBackup message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CreateDatabaseBackup.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.destinationPath != null && $Object.hasOwnProperty.call(message, "destinationPath") && message.destinationPath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.destinationPath);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a CreateDatabaseBackup message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.CreateDatabaseBackup
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateDatabaseBackup & evohime.desktop.v1.CreateDatabaseBackup.$Shape} CreateDatabaseBackup
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CreateDatabaseBackup.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.CreateDatabaseBackup(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.destinationPath = value;
                                else
                                    delete message.destinationPath;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for CreateDatabaseBackup
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.CreateDatabaseBackup
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                CreateDatabaseBackup.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.CreateDatabaseBackup";
                };

                return CreateDatabaseBackup;
            })();

            v1.PrepareDatabaseRestore = (function() {

                /**
                 * Properties of a PrepareDatabaseRestore.
                 * @typedef {Object} evohime.desktop.v1.PrepareDatabaseRestore.$Properties
                 * @property {string|null} [backupPath] PrepareDatabaseRestore backupPath
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a PrepareDatabaseRestore.
                 * @memberof evohime.desktop.v1
                 * @interface IPrepareDatabaseRestore
                 * @augments evohime.desktop.v1.PrepareDatabaseRestore.$Properties
                 * @deprecated Use evohime.desktop.v1.PrepareDatabaseRestore.$Properties instead.
                 */

                /**
                 * Shape of a PrepareDatabaseRestore.
                 * @typedef {evohime.desktop.v1.PrepareDatabaseRestore.$Properties} evohime.desktop.v1.PrepareDatabaseRestore.$Shape
                 */

                /**
                 * Constructs a new PrepareDatabaseRestore.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a PrepareDatabaseRestore.
                 * @constructor
                 * @param {evohime.desktop.v1.PrepareDatabaseRestore.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const PrepareDatabaseRestore = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * PrepareDatabaseRestore backupPath.
                 * @member {string} backupPath
                 * @memberof evohime.desktop.v1.PrepareDatabaseRestore
                 * @instance
                 */
                PrepareDatabaseRestore.prototype.backupPath = "";

                /**
                 * Encodes the specified PrepareDatabaseRestore message. Does not implicitly {@link evohime.desktop.v1.PrepareDatabaseRestore.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.PrepareDatabaseRestore
                 * @static
                 * @param {evohime.desktop.v1.PrepareDatabaseRestore.$Properties} message PrepareDatabaseRestore message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                PrepareDatabaseRestore.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.backupPath != null && $Object.hasOwnProperty.call(message, "backupPath") && message.backupPath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.backupPath);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a PrepareDatabaseRestore message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.PrepareDatabaseRestore
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PrepareDatabaseRestore & evohime.desktop.v1.PrepareDatabaseRestore.$Shape} PrepareDatabaseRestore
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                PrepareDatabaseRestore.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.PrepareDatabaseRestore(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.backupPath = value;
                                else
                                    delete message.backupPath;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for PrepareDatabaseRestore
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.PrepareDatabaseRestore
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                PrepareDatabaseRestore.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.PrepareDatabaseRestore";
                };

                return PrepareDatabaseRestore;
            })();

            v1.RestoreDatabase = (function() {

                /**
                 * Properties of a RestoreDatabase.
                 * @typedef {Object} evohime.desktop.v1.RestoreDatabase.$Properties
                 * @property {string|null} [backupPath] RestoreDatabase backupPath
                 * @property {string|null} [approvalId] RestoreDatabase approvalId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a RestoreDatabase.
                 * @memberof evohime.desktop.v1
                 * @interface IRestoreDatabase
                 * @augments evohime.desktop.v1.RestoreDatabase.$Properties
                 * @deprecated Use evohime.desktop.v1.RestoreDatabase.$Properties instead.
                 */

                /**
                 * Shape of a RestoreDatabase.
                 * @typedef {evohime.desktop.v1.RestoreDatabase.$Properties} evohime.desktop.v1.RestoreDatabase.$Shape
                 */

                /**
                 * Constructs a new RestoreDatabase.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a RestoreDatabase.
                 * @constructor
                 * @param {evohime.desktop.v1.RestoreDatabase.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const RestoreDatabase = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * RestoreDatabase backupPath.
                 * @member {string} backupPath
                 * @memberof evohime.desktop.v1.RestoreDatabase
                 * @instance
                 */
                RestoreDatabase.prototype.backupPath = "";

                /**
                 * RestoreDatabase approvalId.
                 * @member {string} approvalId
                 * @memberof evohime.desktop.v1.RestoreDatabase
                 * @instance
                 */
                RestoreDatabase.prototype.approvalId = "";

                /**
                 * Encodes the specified RestoreDatabase message. Does not implicitly {@link evohime.desktop.v1.RestoreDatabase.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.RestoreDatabase
                 * @static
                 * @param {evohime.desktop.v1.RestoreDatabase.$Properties} message RestoreDatabase message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RestoreDatabase.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.backupPath != null && $Object.hasOwnProperty.call(message, "backupPath") && message.backupPath !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.backupPath);
                    if (message.approvalId != null && $Object.hasOwnProperty.call(message, "approvalId") && message.approvalId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.approvalId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a RestoreDatabase message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.RestoreDatabase
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RestoreDatabase & evohime.desktop.v1.RestoreDatabase.$Shape} RestoreDatabase
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RestoreDatabase.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.RestoreDatabase(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.backupPath = value;
                                else
                                    delete message.backupPath;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.approvalId = value;
                                else
                                    delete message.approvalId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for RestoreDatabase
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.RestoreDatabase
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                RestoreDatabase.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.RestoreDatabase";
                };

                return RestoreDatabase;
            })();

            v1.CancelDatabaseOperation = (function() {

                /**
                 * Properties of a CancelDatabaseOperation.
                 * @typedef {Object} evohime.desktop.v1.CancelDatabaseOperation.$Properties
                 * @property {string|null} [operationId] CancelDatabaseOperation operationId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a CancelDatabaseOperation.
                 * @memberof evohime.desktop.v1
                 * @interface ICancelDatabaseOperation
                 * @augments evohime.desktop.v1.CancelDatabaseOperation.$Properties
                 * @deprecated Use evohime.desktop.v1.CancelDatabaseOperation.$Properties instead.
                 */

                /**
                 * Shape of a CancelDatabaseOperation.
                 * @typedef {evohime.desktop.v1.CancelDatabaseOperation.$Properties} evohime.desktop.v1.CancelDatabaseOperation.$Shape
                 */

                /**
                 * Constructs a new CancelDatabaseOperation.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a CancelDatabaseOperation.
                 * @constructor
                 * @param {evohime.desktop.v1.CancelDatabaseOperation.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const CancelDatabaseOperation = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * CancelDatabaseOperation operationId.
                 * @member {string} operationId
                 * @memberof evohime.desktop.v1.CancelDatabaseOperation
                 * @instance
                 */
                CancelDatabaseOperation.prototype.operationId = "";

                /**
                 * Encodes the specified CancelDatabaseOperation message. Does not implicitly {@link evohime.desktop.v1.CancelDatabaseOperation.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.CancelDatabaseOperation
                 * @static
                 * @param {evohime.desktop.v1.CancelDatabaseOperation.$Properties} message CancelDatabaseOperation message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CancelDatabaseOperation.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.operationId != null && $Object.hasOwnProperty.call(message, "operationId") && message.operationId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.operationId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a CancelDatabaseOperation message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.CancelDatabaseOperation
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CancelDatabaseOperation & evohime.desktop.v1.CancelDatabaseOperation.$Shape} CancelDatabaseOperation
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CancelDatabaseOperation.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.CancelDatabaseOperation(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.operationId = value;
                                else
                                    delete message.operationId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for CancelDatabaseOperation
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.CancelDatabaseOperation
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                CancelDatabaseOperation.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.CancelDatabaseOperation";
                };

                return CancelDatabaseOperation;
            })();

            v1.SaveResearchEvidence = (function() {

                /**
                 * Properties of a SaveResearchEvidence.
                 * @typedef {Object} evohime.desktop.v1.SaveResearchEvidence.$Properties
                 * @property {string|null} [workItemId] SaveResearchEvidence workItemId
                 * @property {string|null} [sourceKind] SaveResearchEvidence sourceKind
                 * @property {string|null} [sourceRef] SaveResearchEvidence sourceRef
                 * @property {string|null} [title] SaveResearchEvidence title
                 * @property {string|null} [publisher] SaveResearchEvidence publisher
                 * @property {string|null} [contentType] SaveResearchEvidence contentType
                 * @property {string|null} [rawExcerpt] SaveResearchEvidence rawExcerpt
                 * @property {number|null} [retrievedAtMs] SaveResearchEvidence retrievedAtMs
                 * @property {number|null} [ttlMs] SaveResearchEvidence ttlMs
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a SaveResearchEvidence.
                 * @memberof evohime.desktop.v1
                 * @interface ISaveResearchEvidence
                 * @augments evohime.desktop.v1.SaveResearchEvidence.$Properties
                 * @deprecated Use evohime.desktop.v1.SaveResearchEvidence.$Properties instead.
                 */

                /**
                 * Shape of a SaveResearchEvidence.
                 * @typedef {evohime.desktop.v1.SaveResearchEvidence.$Properties} evohime.desktop.v1.SaveResearchEvidence.$Shape
                 */

                /**
                 * Constructs a new SaveResearchEvidence.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a SaveResearchEvidence.
                 * @constructor
                 * @param {evohime.desktop.v1.SaveResearchEvidence.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const SaveResearchEvidence = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * SaveResearchEvidence workItemId.
                 * @member {string} workItemId
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.workItemId = "";

                /**
                 * SaveResearchEvidence sourceKind.
                 * @member {string} sourceKind
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.sourceKind = "";

                /**
                 * SaveResearchEvidence sourceRef.
                 * @member {string} sourceRef
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.sourceRef = "";

                /**
                 * SaveResearchEvidence title.
                 * @member {string} title
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.title = "";

                /**
                 * SaveResearchEvidence publisher.
                 * @member {string} publisher
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.publisher = "";

                /**
                 * SaveResearchEvidence contentType.
                 * @member {string} contentType
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.contentType = "";

                /**
                 * SaveResearchEvidence rawExcerpt.
                 * @member {string} rawExcerpt
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.rawExcerpt = "";

                /**
                 * SaveResearchEvidence retrievedAtMs.
                 * @member {number} retrievedAtMs
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.retrievedAtMs = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * SaveResearchEvidence ttlMs.
                 * @member {number} ttlMs
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @instance
                 */
                SaveResearchEvidence.prototype.ttlMs = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Encodes the specified SaveResearchEvidence message. Does not implicitly {@link evohime.desktop.v1.SaveResearchEvidence.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @static
                 * @param {evohime.desktop.v1.SaveResearchEvidence.$Properties} message SaveResearchEvidence message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                SaveResearchEvidence.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.workItemId != null && $Object.hasOwnProperty.call(message, "workItemId") && message.workItemId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.workItemId);
                    if (message.sourceKind != null && $Object.hasOwnProperty.call(message, "sourceKind") && message.sourceKind !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.sourceKind);
                    if (message.sourceRef != null && $Object.hasOwnProperty.call(message, "sourceRef") && message.sourceRef !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.sourceRef);
                    if (message.title != null && $Object.hasOwnProperty.call(message, "title") && message.title !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.title);
                    if (message.publisher != null && $Object.hasOwnProperty.call(message, "publisher") && message.publisher !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.publisher);
                    if (message.contentType != null && $Object.hasOwnProperty.call(message, "contentType") && message.contentType !== "")
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.contentType);
                    if (message.rawExcerpt != null && $Object.hasOwnProperty.call(message, "rawExcerpt") && message.rawExcerpt !== "")
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.rawExcerpt);
                    if (message.retrievedAtMs != null && $Object.hasOwnProperty.call(message, "retrievedAtMs") && (typeof message.retrievedAtMs === "object" ? message.retrievedAtMs.low || message.retrievedAtMs.high : message.retrievedAtMs !== 0))
                        writer.uint32(/* id 8, wireType 0 =*/64).uint64(message.retrievedAtMs);
                    if (message.ttlMs != null && $Object.hasOwnProperty.call(message, "ttlMs") && (typeof message.ttlMs === "object" ? message.ttlMs.low || message.ttlMs.high : message.ttlMs !== 0))
                        writer.uint32(/* id 9, wireType 0 =*/72).uint64(message.ttlMs);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a SaveResearchEvidence message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SaveResearchEvidence & evohime.desktop.v1.SaveResearchEvidence.$Shape} SaveResearchEvidence
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                SaveResearchEvidence.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.SaveResearchEvidence(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workItemId = value;
                                else
                                    delete message.workItemId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.sourceKind = value;
                                else
                                    delete message.sourceKind;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.sourceRef = value;
                                else
                                    delete message.sourceRef;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.title = value;
                                else
                                    delete message.title;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.publisher = value;
                                else
                                    delete message.publisher;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.contentType = value;
                                else
                                    delete message.contentType;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.rawExcerpt = value;
                                else
                                    delete message.rawExcerpt;
                                continue;
                            }
                        case 8: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.retrievedAtMs = value;
                                else
                                    delete message.retrievedAtMs;
                                continue;
                            }
                        case 9: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.ttlMs = value;
                                else
                                    delete message.ttlMs;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for SaveResearchEvidence
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.SaveResearchEvidence
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                SaveResearchEvidence.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.SaveResearchEvidence";
                };

                return SaveResearchEvidence;
            })();

            v1.ListResearchEvidence = (function() {

                /**
                 * Properties of a ListResearchEvidence.
                 * @typedef {Object} evohime.desktop.v1.ListResearchEvidence.$Properties
                 * @property {string|null} [workItemId] ListResearchEvidence workItemId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ListResearchEvidence.
                 * @memberof evohime.desktop.v1
                 * @interface IListResearchEvidence
                 * @augments evohime.desktop.v1.ListResearchEvidence.$Properties
                 * @deprecated Use evohime.desktop.v1.ListResearchEvidence.$Properties instead.
                 */

                /**
                 * Shape of a ListResearchEvidence.
                 * @typedef {evohime.desktop.v1.ListResearchEvidence.$Properties} evohime.desktop.v1.ListResearchEvidence.$Shape
                 */

                /**
                 * Constructs a new ListResearchEvidence.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ListResearchEvidence.
                 * @constructor
                 * @param {evohime.desktop.v1.ListResearchEvidence.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ListResearchEvidence = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ListResearchEvidence workItemId.
                 * @member {string} workItemId
                 * @memberof evohime.desktop.v1.ListResearchEvidence
                 * @instance
                 */
                ListResearchEvidence.prototype.workItemId = "";

                /**
                 * Encodes the specified ListResearchEvidence message. Does not implicitly {@link evohime.desktop.v1.ListResearchEvidence.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ListResearchEvidence
                 * @static
                 * @param {evohime.desktop.v1.ListResearchEvidence.$Properties} message ListResearchEvidence message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ListResearchEvidence.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.workItemId != null && $Object.hasOwnProperty.call(message, "workItemId") && message.workItemId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.workItemId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ListResearchEvidence message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ListResearchEvidence
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListResearchEvidence & evohime.desktop.v1.ListResearchEvidence.$Shape} ListResearchEvidence
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ListResearchEvidence.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ListResearchEvidence(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workItemId = value;
                                else
                                    delete message.workItemId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ListResearchEvidence
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ListResearchEvidence
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ListResearchEvidence.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ListResearchEvidence";
                };

                return ListResearchEvidence;
            })();

            v1.RunResearchFetch = (function() {

                /**
                 * Properties of a RunResearchFetch.
                 * @typedef {Object} evohime.desktop.v1.RunResearchFetch.$Properties
                 * @property {string|null} [workItemId] RunResearchFetch workItemId
                 * @property {string|null} [url] RunResearchFetch url
                 * @property {string|null} [title] RunResearchFetch title
                 * @property {Array.<string>|null} [allowedDomains] RunResearchFetch allowedDomains
                 * @property {number|null} [maxBytes] RunResearchFetch maxBytes
                 * @property {number|null} [maxLatencyMs] RunResearchFetch maxLatencyMs
                 * @property {number|null} [maxCostMicros] RunResearchFetch maxCostMicros
                 * @property {number|null} [ttlMs] RunResearchFetch ttlMs
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a RunResearchFetch.
                 * @memberof evohime.desktop.v1
                 * @interface IRunResearchFetch
                 * @augments evohime.desktop.v1.RunResearchFetch.$Properties
                 * @deprecated Use evohime.desktop.v1.RunResearchFetch.$Properties instead.
                 */

                /**
                 * Shape of a RunResearchFetch.
                 * @typedef {evohime.desktop.v1.RunResearchFetch.$Properties} evohime.desktop.v1.RunResearchFetch.$Shape
                 */

                /**
                 * Constructs a new RunResearchFetch.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a RunResearchFetch.
                 * @constructor
                 * @param {evohime.desktop.v1.RunResearchFetch.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const RunResearchFetch = function (properties) {
                    this.allowedDomains = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * RunResearchFetch workItemId.
                 * @member {string} workItemId
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.workItemId = "";

                /**
                 * RunResearchFetch url.
                 * @member {string} url
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.url = "";

                /**
                 * RunResearchFetch title.
                 * @member {string} title
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.title = "";

                /**
                 * RunResearchFetch allowedDomains.
                 * @member {Array.<string>} allowedDomains
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.allowedDomains = $util.emptyArray;

                /**
                 * RunResearchFetch maxBytes.
                 * @member {number} maxBytes
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.maxBytes = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * RunResearchFetch maxLatencyMs.
                 * @member {number} maxLatencyMs
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.maxLatencyMs = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * RunResearchFetch maxCostMicros.
                 * @member {number} maxCostMicros
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.maxCostMicros = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * RunResearchFetch ttlMs.
                 * @member {number} ttlMs
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @instance
                 */
                RunResearchFetch.prototype.ttlMs = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Encodes the specified RunResearchFetch message. Does not implicitly {@link evohime.desktop.v1.RunResearchFetch.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @static
                 * @param {evohime.desktop.v1.RunResearchFetch.$Properties} message RunResearchFetch message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RunResearchFetch.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.workItemId != null && $Object.hasOwnProperty.call(message, "workItemId") && message.workItemId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.workItemId);
                    if (message.url != null && $Object.hasOwnProperty.call(message, "url") && message.url !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.url);
                    if (message.title != null && $Object.hasOwnProperty.call(message, "title") && message.title !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.title);
                    if (message.allowedDomains != null && message.allowedDomains.length)
                        for (let i = 0; i < message.allowedDomains.length; ++i)
                            writer.uint32(/* id 4, wireType 2 =*/34).string(message.allowedDomains[i]);
                    if (message.maxBytes != null && $Object.hasOwnProperty.call(message, "maxBytes") && (typeof message.maxBytes === "object" ? message.maxBytes.low || message.maxBytes.high : message.maxBytes !== 0))
                        writer.uint32(/* id 5, wireType 0 =*/40).uint64(message.maxBytes);
                    if (message.maxLatencyMs != null && $Object.hasOwnProperty.call(message, "maxLatencyMs") && (typeof message.maxLatencyMs === "object" ? message.maxLatencyMs.low || message.maxLatencyMs.high : message.maxLatencyMs !== 0))
                        writer.uint32(/* id 6, wireType 0 =*/48).uint64(message.maxLatencyMs);
                    if (message.maxCostMicros != null && $Object.hasOwnProperty.call(message, "maxCostMicros") && (typeof message.maxCostMicros === "object" ? message.maxCostMicros.low || message.maxCostMicros.high : message.maxCostMicros !== 0))
                        writer.uint32(/* id 7, wireType 0 =*/56).uint64(message.maxCostMicros);
                    if (message.ttlMs != null && $Object.hasOwnProperty.call(message, "ttlMs") && (typeof message.ttlMs === "object" ? message.ttlMs.low || message.ttlMs.high : message.ttlMs !== 0))
                        writer.uint32(/* id 8, wireType 0 =*/64).uint64(message.ttlMs);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a RunResearchFetch message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RunResearchFetch & evohime.desktop.v1.RunResearchFetch.$Shape} RunResearchFetch
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RunResearchFetch.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.RunResearchFetch(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.workItemId = value;
                                else
                                    delete message.workItemId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.url = value;
                                else
                                    delete message.url;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.title = value;
                                else
                                    delete message.title;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.allowedDomains && message.allowedDomains.length))
                                    message.allowedDomains = [];
                                message.allowedDomains.push(reader.stringVerify());
                                continue;
                            }
                        case 5: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.maxBytes = value;
                                else
                                    delete message.maxBytes;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.maxLatencyMs = value;
                                else
                                    delete message.maxLatencyMs;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.maxCostMicros = value;
                                else
                                    delete message.maxCostMicros;
                                continue;
                            }
                        case 8: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.ttlMs = value;
                                else
                                    delete message.ttlMs;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for RunResearchFetch
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.RunResearchFetch
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                RunResearchFetch.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.RunResearchFetch";
                };

                return RunResearchFetch;
            })();

            v1.CreateMemory = (function() {

                /**
                 * Properties of a CreateMemory.
                 * @typedef {Object} evohime.desktop.v1.CreateMemory.$Properties
                 * @property {string|null} [scopeKind] CreateMemory scopeKind
                 * @property {string|null} [projectId] CreateMemory projectId
                 * @property {string|null} [secondaryId] CreateMemory secondaryId
                 * @property {string|null} [title] CreateMemory title
                 * @property {string|null} [content] CreateMemory content
                 * @property {string|null} [provenanceKind] CreateMemory provenanceKind
                 * @property {string|null} [provenanceId] CreateMemory provenanceId
                 * @property {string|null} [provenanceLocator] CreateMemory provenanceLocator
                 * @property {string|null} [privacy] CreateMemory privacy
                 * @property {number|null} [ttlMs] CreateMemory ttlMs
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a CreateMemory.
                 * @memberof evohime.desktop.v1
                 * @interface ICreateMemory
                 * @augments evohime.desktop.v1.CreateMemory.$Properties
                 * @deprecated Use evohime.desktop.v1.CreateMemory.$Properties instead.
                 */

                /**
                 * Shape of a CreateMemory.
                 * @typedef {evohime.desktop.v1.CreateMemory.$Properties} evohime.desktop.v1.CreateMemory.$Shape
                 */

                /**
                 * Constructs a new CreateMemory.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a CreateMemory.
                 * @constructor
                 * @param {evohime.desktop.v1.CreateMemory.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const CreateMemory = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * CreateMemory scopeKind.
                 * @member {string} scopeKind
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.scopeKind = "";

                /**
                 * CreateMemory projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.projectId = "";

                /**
                 * CreateMemory secondaryId.
                 * @member {string} secondaryId
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.secondaryId = "";

                /**
                 * CreateMemory title.
                 * @member {string} title
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.title = "";

                /**
                 * CreateMemory content.
                 * @member {string} content
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.content = "";

                /**
                 * CreateMemory provenanceKind.
                 * @member {string} provenanceKind
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.provenanceKind = "";

                /**
                 * CreateMemory provenanceId.
                 * @member {string} provenanceId
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.provenanceId = "";

                /**
                 * CreateMemory provenanceLocator.
                 * @member {string} provenanceLocator
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.provenanceLocator = "";

                /**
                 * CreateMemory privacy.
                 * @member {string} privacy
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.privacy = "";

                /**
                 * CreateMemory ttlMs.
                 * @member {number} ttlMs
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @instance
                 */
                CreateMemory.prototype.ttlMs = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Encodes the specified CreateMemory message. Does not implicitly {@link evohime.desktop.v1.CreateMemory.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @static
                 * @param {evohime.desktop.v1.CreateMemory.$Properties} message CreateMemory message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CreateMemory.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.scopeKind != null && $Object.hasOwnProperty.call(message, "scopeKind") && message.scopeKind !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.scopeKind);
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.projectId);
                    if (message.secondaryId != null && $Object.hasOwnProperty.call(message, "secondaryId") && message.secondaryId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.secondaryId);
                    if (message.title != null && $Object.hasOwnProperty.call(message, "title") && message.title !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.title);
                    if (message.content != null && $Object.hasOwnProperty.call(message, "content") && message.content !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.content);
                    if (message.provenanceKind != null && $Object.hasOwnProperty.call(message, "provenanceKind") && message.provenanceKind !== "")
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.provenanceKind);
                    if (message.provenanceId != null && $Object.hasOwnProperty.call(message, "provenanceId") && message.provenanceId !== "")
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.provenanceId);
                    if (message.provenanceLocator != null && $Object.hasOwnProperty.call(message, "provenanceLocator") && message.provenanceLocator !== "")
                        writer.uint32(/* id 8, wireType 2 =*/66).string(message.provenanceLocator);
                    if (message.privacy != null && $Object.hasOwnProperty.call(message, "privacy") && message.privacy !== "")
                        writer.uint32(/* id 9, wireType 2 =*/74).string(message.privacy);
                    if (message.ttlMs != null && $Object.hasOwnProperty.call(message, "ttlMs") && (typeof message.ttlMs === "object" ? message.ttlMs.low || message.ttlMs.high : message.ttlMs !== 0))
                        writer.uint32(/* id 10, wireType 0 =*/80).uint64(message.ttlMs);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a CreateMemory message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateMemory & evohime.desktop.v1.CreateMemory.$Shape} CreateMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CreateMemory.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.CreateMemory(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.scopeKind = value;
                                else
                                    delete message.scopeKind;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.secondaryId = value;
                                else
                                    delete message.secondaryId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.title = value;
                                else
                                    delete message.title;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.content = value;
                                else
                                    delete message.content;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.provenanceKind = value;
                                else
                                    delete message.provenanceKind;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.provenanceId = value;
                                else
                                    delete message.provenanceId;
                                continue;
                            }
                        case 8: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.provenanceLocator = value;
                                else
                                    delete message.provenanceLocator;
                                continue;
                            }
                        case 9: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.privacy = value;
                                else
                                    delete message.privacy;
                                continue;
                            }
                        case 10: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.ttlMs = value;
                                else
                                    delete message.ttlMs;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for CreateMemory
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.CreateMemory
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                CreateMemory.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.CreateMemory";
                };

                return CreateMemory;
            })();

            v1.ListMemory = (function() {

                /**
                 * Properties of a ListMemory.
                 * @typedef {Object} evohime.desktop.v1.ListMemory.$Properties
                 * @property {string|null} [scopeKind] ListMemory scopeKind
                 * @property {string|null} [projectId] ListMemory projectId
                 * @property {string|null} [secondaryId] ListMemory secondaryId
                 * @property {boolean|null} [includeArchived] ListMemory includeArchived
                 * @property {number|null} [limit] ListMemory limit
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ListMemory.
                 * @memberof evohime.desktop.v1
                 * @interface IListMemory
                 * @augments evohime.desktop.v1.ListMemory.$Properties
                 * @deprecated Use evohime.desktop.v1.ListMemory.$Properties instead.
                 */

                /**
                 * Shape of a ListMemory.
                 * @typedef {evohime.desktop.v1.ListMemory.$Properties} evohime.desktop.v1.ListMemory.$Shape
                 */

                /**
                 * Constructs a new ListMemory.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ListMemory.
                 * @constructor
                 * @param {evohime.desktop.v1.ListMemory.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ListMemory = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ListMemory scopeKind.
                 * @member {string} scopeKind
                 * @memberof evohime.desktop.v1.ListMemory
                 * @instance
                 */
                ListMemory.prototype.scopeKind = "";

                /**
                 * ListMemory projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.ListMemory
                 * @instance
                 */
                ListMemory.prototype.projectId = "";

                /**
                 * ListMemory secondaryId.
                 * @member {string} secondaryId
                 * @memberof evohime.desktop.v1.ListMemory
                 * @instance
                 */
                ListMemory.prototype.secondaryId = "";

                /**
                 * ListMemory includeArchived.
                 * @member {boolean} includeArchived
                 * @memberof evohime.desktop.v1.ListMemory
                 * @instance
                 */
                ListMemory.prototype.includeArchived = false;

                /**
                 * ListMemory limit.
                 * @member {number} limit
                 * @memberof evohime.desktop.v1.ListMemory
                 * @instance
                 */
                ListMemory.prototype.limit = 0;

                /**
                 * Encodes the specified ListMemory message. Does not implicitly {@link evohime.desktop.v1.ListMemory.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ListMemory
                 * @static
                 * @param {evohime.desktop.v1.ListMemory.$Properties} message ListMemory message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ListMemory.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.scopeKind != null && $Object.hasOwnProperty.call(message, "scopeKind") && message.scopeKind !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.scopeKind);
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.projectId);
                    if (message.secondaryId != null && $Object.hasOwnProperty.call(message, "secondaryId") && message.secondaryId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.secondaryId);
                    if (message.includeArchived != null && $Object.hasOwnProperty.call(message, "includeArchived") && message.includeArchived !== false)
                        writer.uint32(/* id 4, wireType 0 =*/32).bool(message.includeArchived);
                    if (message.limit != null && $Object.hasOwnProperty.call(message, "limit") && message.limit !== 0)
                        writer.uint32(/* id 5, wireType 0 =*/40).uint32(message.limit);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ListMemory message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ListMemory
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListMemory & evohime.desktop.v1.ListMemory.$Shape} ListMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ListMemory.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ListMemory(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.scopeKind = value;
                                else
                                    delete message.scopeKind;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.secondaryId = value;
                                else
                                    delete message.secondaryId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.bool())
                                    message.includeArchived = value;
                                else
                                    delete message.includeArchived;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.limit = value;
                                else
                                    delete message.limit;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ListMemory
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ListMemory
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ListMemory.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ListMemory";
                };

                return ListMemory;
            })();

            v1.SearchMemory = (function() {

                /**
                 * Properties of a SearchMemory.
                 * @typedef {Object} evohime.desktop.v1.SearchMemory.$Properties
                 * @property {string|null} [scopeKind] SearchMemory scopeKind
                 * @property {string|null} [projectId] SearchMemory projectId
                 * @property {string|null} [secondaryId] SearchMemory secondaryId
                 * @property {string|null} [query] SearchMemory query
                 * @property {number|null} [limit] SearchMemory limit
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a SearchMemory.
                 * @memberof evohime.desktop.v1
                 * @interface ISearchMemory
                 * @augments evohime.desktop.v1.SearchMemory.$Properties
                 * @deprecated Use evohime.desktop.v1.SearchMemory.$Properties instead.
                 */

                /**
                 * Shape of a SearchMemory.
                 * @typedef {evohime.desktop.v1.SearchMemory.$Properties} evohime.desktop.v1.SearchMemory.$Shape
                 */

                /**
                 * Constructs a new SearchMemory.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a SearchMemory.
                 * @constructor
                 * @param {evohime.desktop.v1.SearchMemory.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const SearchMemory = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * SearchMemory scopeKind.
                 * @member {string} scopeKind
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @instance
                 */
                SearchMemory.prototype.scopeKind = "";

                /**
                 * SearchMemory projectId.
                 * @member {string} projectId
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @instance
                 */
                SearchMemory.prototype.projectId = "";

                /**
                 * SearchMemory secondaryId.
                 * @member {string} secondaryId
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @instance
                 */
                SearchMemory.prototype.secondaryId = "";

                /**
                 * SearchMemory query.
                 * @member {string} query
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @instance
                 */
                SearchMemory.prototype.query = "";

                /**
                 * SearchMemory limit.
                 * @member {number} limit
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @instance
                 */
                SearchMemory.prototype.limit = 0;

                /**
                 * Encodes the specified SearchMemory message. Does not implicitly {@link evohime.desktop.v1.SearchMemory.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @static
                 * @param {evohime.desktop.v1.SearchMemory.$Properties} message SearchMemory message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                SearchMemory.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.scopeKind != null && $Object.hasOwnProperty.call(message, "scopeKind") && message.scopeKind !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.scopeKind);
                    if (message.projectId != null && $Object.hasOwnProperty.call(message, "projectId") && message.projectId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.projectId);
                    if (message.secondaryId != null && $Object.hasOwnProperty.call(message, "secondaryId") && message.secondaryId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.secondaryId);
                    if (message.query != null && $Object.hasOwnProperty.call(message, "query") && message.query !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.query);
                    if (message.limit != null && $Object.hasOwnProperty.call(message, "limit") && message.limit !== 0)
                        writer.uint32(/* id 5, wireType 0 =*/40).uint32(message.limit);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a SearchMemory message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SearchMemory & evohime.desktop.v1.SearchMemory.$Shape} SearchMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                SearchMemory.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.SearchMemory(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.scopeKind = value;
                                else
                                    delete message.scopeKind;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.projectId = value;
                                else
                                    delete message.projectId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.secondaryId = value;
                                else
                                    delete message.secondaryId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.query = value;
                                else
                                    delete message.query;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.limit = value;
                                else
                                    delete message.limit;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for SearchMemory
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.SearchMemory
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                SearchMemory.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.SearchMemory";
                };

                return SearchMemory;
            })();

            v1.ArchiveMemory = (function() {

                /**
                 * Properties of an ArchiveMemory.
                 * @typedef {Object} evohime.desktop.v1.ArchiveMemory.$Properties
                 * @property {string|null} [id] ArchiveMemory id
                 * @property {string|null} [approvalId] ArchiveMemory approvalId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an ArchiveMemory.
                 * @memberof evohime.desktop.v1
                 * @interface IArchiveMemory
                 * @augments evohime.desktop.v1.ArchiveMemory.$Properties
                 * @deprecated Use evohime.desktop.v1.ArchiveMemory.$Properties instead.
                 */

                /**
                 * Shape of an ArchiveMemory.
                 * @typedef {evohime.desktop.v1.ArchiveMemory.$Properties} evohime.desktop.v1.ArchiveMemory.$Shape
                 */

                /**
                 * Constructs a new ArchiveMemory.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an ArchiveMemory.
                 * @constructor
                 * @param {evohime.desktop.v1.ArchiveMemory.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ArchiveMemory = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ArchiveMemory id.
                 * @member {string} id
                 * @memberof evohime.desktop.v1.ArchiveMemory
                 * @instance
                 */
                ArchiveMemory.prototype.id = "";

                /**
                 * ArchiveMemory approvalId.
                 * @member {string} approvalId
                 * @memberof evohime.desktop.v1.ArchiveMemory
                 * @instance
                 */
                ArchiveMemory.prototype.approvalId = "";

                /**
                 * Encodes the specified ArchiveMemory message. Does not implicitly {@link evohime.desktop.v1.ArchiveMemory.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ArchiveMemory
                 * @static
                 * @param {evohime.desktop.v1.ArchiveMemory.$Properties} message ArchiveMemory message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ArchiveMemory.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.id != null && $Object.hasOwnProperty.call(message, "id") && message.id !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.id);
                    if (message.approvalId != null && $Object.hasOwnProperty.call(message, "approvalId") && message.approvalId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.approvalId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an ArchiveMemory message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ArchiveMemory
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ArchiveMemory & evohime.desktop.v1.ArchiveMemory.$Shape} ArchiveMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ArchiveMemory.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ArchiveMemory(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.id = value;
                                else
                                    delete message.id;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.approvalId = value;
                                else
                                    delete message.approvalId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ArchiveMemory
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ArchiveMemory
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ArchiveMemory.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ArchiveMemory";
                };

                return ArchiveMemory;
            })();

            v1.ForgetMemory = (function() {

                /**
                 * Properties of a ForgetMemory.
                 * @typedef {Object} evohime.desktop.v1.ForgetMemory.$Properties
                 * @property {string|null} [id] ForgetMemory id
                 * @property {string|null} [approvalId] ForgetMemory approvalId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ForgetMemory.
                 * @memberof evohime.desktop.v1
                 * @interface IForgetMemory
                 * @augments evohime.desktop.v1.ForgetMemory.$Properties
                 * @deprecated Use evohime.desktop.v1.ForgetMemory.$Properties instead.
                 */

                /**
                 * Shape of a ForgetMemory.
                 * @typedef {evohime.desktop.v1.ForgetMemory.$Properties} evohime.desktop.v1.ForgetMemory.$Shape
                 */

                /**
                 * Constructs a new ForgetMemory.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ForgetMemory.
                 * @constructor
                 * @param {evohime.desktop.v1.ForgetMemory.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ForgetMemory = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ForgetMemory id.
                 * @member {string} id
                 * @memberof evohime.desktop.v1.ForgetMemory
                 * @instance
                 */
                ForgetMemory.prototype.id = "";

                /**
                 * ForgetMemory approvalId.
                 * @member {string} approvalId
                 * @memberof evohime.desktop.v1.ForgetMemory
                 * @instance
                 */
                ForgetMemory.prototype.approvalId = "";

                /**
                 * Encodes the specified ForgetMemory message. Does not implicitly {@link evohime.desktop.v1.ForgetMemory.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ForgetMemory
                 * @static
                 * @param {evohime.desktop.v1.ForgetMemory.$Properties} message ForgetMemory message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ForgetMemory.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.id != null && $Object.hasOwnProperty.call(message, "id") && message.id !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.id);
                    if (message.approvalId != null && $Object.hasOwnProperty.call(message, "approvalId") && message.approvalId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.approvalId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ForgetMemory message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ForgetMemory
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ForgetMemory & evohime.desktop.v1.ForgetMemory.$Shape} ForgetMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ForgetMemory.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ForgetMemory(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.id = value;
                                else
                                    delete message.id;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.approvalId = value;
                                else
                                    delete message.approvalId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ForgetMemory
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ForgetMemory
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ForgetMemory.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ForgetMemory";
                };

                return ForgetMemory;
            })();

            v1.InstallCapability = (function() {

                /**
                 * Properties of an InstallCapability.
                 * @typedef {Object} evohime.desktop.v1.InstallCapability.$Properties
                 * @property {string|null} [manifestJson] InstallCapability manifestJson
                 * @property {string|null} [installSource] InstallCapability installSource
                 * @property {string|null} [sourcePath] InstallCapability sourcePath
                 * @property {string|null} [expectedContentHash] InstallCapability expectedContentHash
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an InstallCapability.
                 * @memberof evohime.desktop.v1
                 * @interface IInstallCapability
                 * @augments evohime.desktop.v1.InstallCapability.$Properties
                 * @deprecated Use evohime.desktop.v1.InstallCapability.$Properties instead.
                 */

                /**
                 * Shape of an InstallCapability.
                 * @typedef {evohime.desktop.v1.InstallCapability.$Properties} evohime.desktop.v1.InstallCapability.$Shape
                 */

                /**
                 * Constructs a new InstallCapability.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an InstallCapability.
                 * @constructor
                 * @param {evohime.desktop.v1.InstallCapability.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const InstallCapability = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * InstallCapability manifestJson.
                 * @member {string} manifestJson
                 * @memberof evohime.desktop.v1.InstallCapability
                 * @instance
                 */
                InstallCapability.prototype.manifestJson = "";

                /**
                 * InstallCapability installSource.
                 * @member {string} installSource
                 * @memberof evohime.desktop.v1.InstallCapability
                 * @instance
                 */
                InstallCapability.prototype.installSource = "";

                /**
                 * InstallCapability sourcePath.
                 * @member {string} sourcePath
                 * @memberof evohime.desktop.v1.InstallCapability
                 * @instance
                 */
                InstallCapability.prototype.sourcePath = "";

                /**
                 * InstallCapability expectedContentHash.
                 * @member {string} expectedContentHash
                 * @memberof evohime.desktop.v1.InstallCapability
                 * @instance
                 */
                InstallCapability.prototype.expectedContentHash = "";

                /**
                 * Encodes the specified InstallCapability message. Does not implicitly {@link evohime.desktop.v1.InstallCapability.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.InstallCapability
                 * @static
                 * @param {evohime.desktop.v1.InstallCapability.$Properties} message InstallCapability message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InstallCapability.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.manifestJson != null && $Object.hasOwnProperty.call(message, "manifestJson") && message.manifestJson !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.manifestJson);
                    if (message.installSource != null && $Object.hasOwnProperty.call(message, "installSource") && message.installSource !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.installSource);
                    if (message.sourcePath != null && $Object.hasOwnProperty.call(message, "sourcePath") && message.sourcePath !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.sourcePath);
                    if (message.expectedContentHash != null && $Object.hasOwnProperty.call(message, "expectedContentHash") && message.expectedContentHash !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.expectedContentHash);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an InstallCapability message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.InstallCapability
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.InstallCapability & evohime.desktop.v1.InstallCapability.$Shape} InstallCapability
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InstallCapability.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.InstallCapability(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.manifestJson = value;
                                else
                                    delete message.manifestJson;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.installSource = value;
                                else
                                    delete message.installSource;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.sourcePath = value;
                                else
                                    delete message.sourcePath;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.expectedContentHash = value;
                                else
                                    delete message.expectedContentHash;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for InstallCapability
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.InstallCapability
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                InstallCapability.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.InstallCapability";
                };

                return InstallCapability;
            })();

            v1.ListCapabilities = (function() {

                /**
                 * Properties of a ListCapabilities.
                 * @typedef {Object} evohime.desktop.v1.ListCapabilities.$Properties
                 * @property {number|null} [limit] ListCapabilities limit
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ListCapabilities.
                 * @memberof evohime.desktop.v1
                 * @interface IListCapabilities
                 * @augments evohime.desktop.v1.ListCapabilities.$Properties
                 * @deprecated Use evohime.desktop.v1.ListCapabilities.$Properties instead.
                 */

                /**
                 * Shape of a ListCapabilities.
                 * @typedef {evohime.desktop.v1.ListCapabilities.$Properties} evohime.desktop.v1.ListCapabilities.$Shape
                 */

                /**
                 * Constructs a new ListCapabilities.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ListCapabilities.
                 * @constructor
                 * @param {evohime.desktop.v1.ListCapabilities.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ListCapabilities = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ListCapabilities limit.
                 * @member {number} limit
                 * @memberof evohime.desktop.v1.ListCapabilities
                 * @instance
                 */
                ListCapabilities.prototype.limit = 0;

                /**
                 * Encodes the specified ListCapabilities message. Does not implicitly {@link evohime.desktop.v1.ListCapabilities.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ListCapabilities
                 * @static
                 * @param {evohime.desktop.v1.ListCapabilities.$Properties} message ListCapabilities message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ListCapabilities.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.limit != null && $Object.hasOwnProperty.call(message, "limit") && message.limit !== 0)
                        writer.uint32(/* id 1, wireType 0 =*/8).uint32(message.limit);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ListCapabilities message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ListCapabilities
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListCapabilities & evohime.desktop.v1.ListCapabilities.$Shape} ListCapabilities
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ListCapabilities.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ListCapabilities(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.limit = value;
                                else
                                    delete message.limit;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ListCapabilities
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ListCapabilities
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ListCapabilities.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ListCapabilities";
                };

                return ListCapabilities;
            })();

            v1.MatchCapabilities = (function() {

                /**
                 * Properties of a MatchCapabilities.
                 * @typedef {Object} evohime.desktop.v1.MatchCapabilities.$Properties
                 * @property {string|null} [intent] MatchCapabilities intent
                 * @property {Array.<string>|null} [requiredTools] MatchCapabilities requiredTools
                 * @property {Array.<string>|null} [requiredDomains] MatchCapabilities requiredDomains
                 * @property {string|null} [requestedRisk] MatchCapabilities requestedRisk
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a MatchCapabilities.
                 * @memberof evohime.desktop.v1
                 * @interface IMatchCapabilities
                 * @augments evohime.desktop.v1.MatchCapabilities.$Properties
                 * @deprecated Use evohime.desktop.v1.MatchCapabilities.$Properties instead.
                 */

                /**
                 * Shape of a MatchCapabilities.
                 * @typedef {evohime.desktop.v1.MatchCapabilities.$Properties} evohime.desktop.v1.MatchCapabilities.$Shape
                 */

                /**
                 * Constructs a new MatchCapabilities.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a MatchCapabilities.
                 * @constructor
                 * @param {evohime.desktop.v1.MatchCapabilities.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const MatchCapabilities = function (properties) {
                    this.requiredTools = [];
                    this.requiredDomains = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * MatchCapabilities intent.
                 * @member {string} intent
                 * @memberof evohime.desktop.v1.MatchCapabilities
                 * @instance
                 */
                MatchCapabilities.prototype.intent = "";

                /**
                 * MatchCapabilities requiredTools.
                 * @member {Array.<string>} requiredTools
                 * @memberof evohime.desktop.v1.MatchCapabilities
                 * @instance
                 */
                MatchCapabilities.prototype.requiredTools = $util.emptyArray;

                /**
                 * MatchCapabilities requiredDomains.
                 * @member {Array.<string>} requiredDomains
                 * @memberof evohime.desktop.v1.MatchCapabilities
                 * @instance
                 */
                MatchCapabilities.prototype.requiredDomains = $util.emptyArray;

                /**
                 * MatchCapabilities requestedRisk.
                 * @member {string} requestedRisk
                 * @memberof evohime.desktop.v1.MatchCapabilities
                 * @instance
                 */
                MatchCapabilities.prototype.requestedRisk = "";

                /**
                 * Encodes the specified MatchCapabilities message. Does not implicitly {@link evohime.desktop.v1.MatchCapabilities.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.MatchCapabilities
                 * @static
                 * @param {evohime.desktop.v1.MatchCapabilities.$Properties} message MatchCapabilities message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                MatchCapabilities.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.intent != null && $Object.hasOwnProperty.call(message, "intent") && message.intent !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.intent);
                    if (message.requiredTools != null && message.requiredTools.length)
                        for (let i = 0; i < message.requiredTools.length; ++i)
                            writer.uint32(/* id 2, wireType 2 =*/18).string(message.requiredTools[i]);
                    if (message.requiredDomains != null && message.requiredDomains.length)
                        for (let i = 0; i < message.requiredDomains.length; ++i)
                            writer.uint32(/* id 3, wireType 2 =*/26).string(message.requiredDomains[i]);
                    if (message.requestedRisk != null && $Object.hasOwnProperty.call(message, "requestedRisk") && message.requestedRisk !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.requestedRisk);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a MatchCapabilities message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.MatchCapabilities
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.MatchCapabilities & evohime.desktop.v1.MatchCapabilities.$Shape} MatchCapabilities
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                MatchCapabilities.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.MatchCapabilities(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.intent = value;
                                else
                                    delete message.intent;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.requiredTools && message.requiredTools.length))
                                    message.requiredTools = [];
                                message.requiredTools.push(reader.stringVerify());
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.requiredDomains && message.requiredDomains.length))
                                    message.requiredDomains = [];
                                message.requiredDomains.push(reader.stringVerify());
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.requestedRisk = value;
                                else
                                    delete message.requestedRisk;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for MatchCapabilities
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.MatchCapabilities
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                MatchCapabilities.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.MatchCapabilities";
                };

                return MatchCapabilities;
            })();

            v1.RemoveCapability = (function() {

                /**
                 * Properties of a RemoveCapability.
                 * @typedef {Object} evohime.desktop.v1.RemoveCapability.$Properties
                 * @property {string|null} [id] RemoveCapability id
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a RemoveCapability.
                 * @memberof evohime.desktop.v1
                 * @interface IRemoveCapability
                 * @augments evohime.desktop.v1.RemoveCapability.$Properties
                 * @deprecated Use evohime.desktop.v1.RemoveCapability.$Properties instead.
                 */

                /**
                 * Shape of a RemoveCapability.
                 * @typedef {evohime.desktop.v1.RemoveCapability.$Properties} evohime.desktop.v1.RemoveCapability.$Shape
                 */

                /**
                 * Constructs a new RemoveCapability.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a RemoveCapability.
                 * @constructor
                 * @param {evohime.desktop.v1.RemoveCapability.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const RemoveCapability = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * RemoveCapability id.
                 * @member {string} id
                 * @memberof evohime.desktop.v1.RemoveCapability
                 * @instance
                 */
                RemoveCapability.prototype.id = "";

                /**
                 * Encodes the specified RemoveCapability message. Does not implicitly {@link evohime.desktop.v1.RemoveCapability.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.RemoveCapability
                 * @static
                 * @param {evohime.desktop.v1.RemoveCapability.$Properties} message RemoveCapability message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RemoveCapability.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.id != null && $Object.hasOwnProperty.call(message, "id") && message.id !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.id);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a RemoveCapability message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.RemoveCapability
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RemoveCapability & evohime.desktop.v1.RemoveCapability.$Shape} RemoveCapability
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RemoveCapability.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.RemoveCapability(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.id = value;
                                else
                                    delete message.id;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for RemoveCapability
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.RemoveCapability
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                RemoveCapability.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.RemoveCapability";
                };

                return RemoveCapability;
            })();

            v1.RequestChildHandoff = (function() {

                /**
                 * Properties of a RequestChildHandoff.
                 * @typedef {Object} evohime.desktop.v1.RequestChildHandoff.$Properties
                 * @property {string|null} [handoffId] RequestChildHandoff handoffId
                 * @property {string|null} [taskId] RequestChildHandoff taskId
                 * @property {string|null} [kind] RequestChildHandoff kind
                 * @property {string|null} [fromRole] RequestChildHandoff fromRole
                 * @property {string|null} [fromName] RequestChildHandoff fromName
                 * @property {string|null} [toRole] RequestChildHandoff toRole
                 * @property {string|null} [toName] RequestChildHandoff toName
                 * @property {string|null} [purpose] RequestChildHandoff purpose
                 * @property {Object.<string,string>|null} [payload] RequestChildHandoff payload
                 * @property {number|null} [sequence] RequestChildHandoff sequence
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a RequestChildHandoff.
                 * @memberof evohime.desktop.v1
                 * @interface IRequestChildHandoff
                 * @augments evohime.desktop.v1.RequestChildHandoff.$Properties
                 * @deprecated Use evohime.desktop.v1.RequestChildHandoff.$Properties instead.
                 */

                /**
                 * Shape of a RequestChildHandoff.
                 * @typedef {evohime.desktop.v1.RequestChildHandoff.$Properties} evohime.desktop.v1.RequestChildHandoff.$Shape
                 */

                /**
                 * Constructs a new RequestChildHandoff.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a RequestChildHandoff.
                 * @constructor
                 * @param {evohime.desktop.v1.RequestChildHandoff.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const RequestChildHandoff = function (properties) {
                    this.payload = {};
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * RequestChildHandoff handoffId.
                 * @member {string} handoffId
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.handoffId = "";

                /**
                 * RequestChildHandoff taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.taskId = "";

                /**
                 * RequestChildHandoff kind.
                 * @member {string} kind
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.kind = "";

                /**
                 * RequestChildHandoff fromRole.
                 * @member {string} fromRole
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.fromRole = "";

                /**
                 * RequestChildHandoff fromName.
                 * @member {string} fromName
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.fromName = "";

                /**
                 * RequestChildHandoff toRole.
                 * @member {string} toRole
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.toRole = "";

                /**
                 * RequestChildHandoff toName.
                 * @member {string} toName
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.toName = "";

                /**
                 * RequestChildHandoff purpose.
                 * @member {string} purpose
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.purpose = "";

                /**
                 * RequestChildHandoff payload.
                 * @member {Object.<string,string>} payload
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.payload = $util.emptyObject;

                /**
                 * RequestChildHandoff sequence.
                 * @member {number} sequence
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @instance
                 */
                RequestChildHandoff.prototype.sequence = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Encodes the specified RequestChildHandoff message. Does not implicitly {@link evohime.desktop.v1.RequestChildHandoff.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @static
                 * @param {evohime.desktop.v1.RequestChildHandoff.$Properties} message RequestChildHandoff message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RequestChildHandoff.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.handoffId != null && $Object.hasOwnProperty.call(message, "handoffId") && message.handoffId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.handoffId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.taskId);
                    if (message.kind != null && $Object.hasOwnProperty.call(message, "kind") && message.kind !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.kind);
                    if (message.fromRole != null && $Object.hasOwnProperty.call(message, "fromRole") && message.fromRole !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.fromRole);
                    if (message.fromName != null && $Object.hasOwnProperty.call(message, "fromName") && message.fromName !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.fromName);
                    if (message.toRole != null && $Object.hasOwnProperty.call(message, "toRole") && message.toRole !== "")
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.toRole);
                    if (message.toName != null && $Object.hasOwnProperty.call(message, "toName") && message.toName !== "")
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.toName);
                    if (message.purpose != null && $Object.hasOwnProperty.call(message, "purpose") && message.purpose !== "")
                        writer.uint32(/* id 8, wireType 2 =*/66).string(message.purpose);
                    if (message.payload != null && $Object.hasOwnProperty.call(message, "payload"))
                        for (let keys = $Object.keys(message.payload), i = 0; i < keys.length; ++i)
                            writer.uint32(/* id 9, wireType 2 =*/74).fork().uint32(/* id 1, wireType 2 =*/10).string(keys[i]).uint32(/* id 2, wireType 2 =*/18).string(message.payload[keys[i]]).ldelim();
                    if (message.sequence != null && $Object.hasOwnProperty.call(message, "sequence") && (typeof message.sequence === "object" ? message.sequence.low || message.sequence.high : message.sequence !== 0))
                        writer.uint32(/* id 10, wireType 0 =*/80).uint64(message.sequence);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a RequestChildHandoff message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RequestChildHandoff & evohime.desktop.v1.RequestChildHandoff.$Shape} RequestChildHandoff
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RequestChildHandoff.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.RequestChildHandoff(), key, value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.handoffId = value;
                                else
                                    delete message.handoffId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.kind = value;
                                else
                                    delete message.kind;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.fromRole = value;
                                else
                                    delete message.fromRole;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.fromName = value;
                                else
                                    delete message.fromName;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.toRole = value;
                                else
                                    delete message.toRole;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.toName = value;
                                else
                                    delete message.toName;
                                continue;
                            }
                        case 8: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.purpose = value;
                                else
                                    delete message.purpose;
                                continue;
                            }
                        case 9: {
                                if (wireType !== 2)
                                    break;
                                if (message.payload === $util.emptyObject)
                                    message.payload = {};
                                let end2 = reader.uint32() + reader.pos;
                                key = "";
                                value = "";
                                while (reader.pos < end2) {
                                    let tag2 = reader.tag();
                                    wireType = tag2 & 7;
                                    switch (tag2 >>>= 3) {
                                    case 1:
                                        if (wireType !== 2)
                                            break;
                                        key = reader.stringVerify();
                                        continue;
                                    case 2:
                                        if (wireType !== 2)
                                            break;
                                        value = reader.stringVerify();
                                        continue;
                                    }
                                    reader.skipType(wireType, _depth, tag2);
                                }
                                if (key === "__proto__")
                                    $util.makeProp(message.payload, key);
                                message.payload[key] = value;
                                continue;
                            }
                        case 10: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.sequence = value;
                                else
                                    delete message.sequence;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for RequestChildHandoff
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.RequestChildHandoff
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                RequestChildHandoff.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.RequestChildHandoff";
                };

                return RequestChildHandoff;
            })();

            v1.ListChildHandoffs = (function() {

                /**
                 * Properties of a ListChildHandoffs.
                 * @typedef {Object} evohime.desktop.v1.ListChildHandoffs.$Properties
                 * @property {string|null} [taskId] ListChildHandoffs taskId
                 * @property {number|null} [limit] ListChildHandoffs limit
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ListChildHandoffs.
                 * @memberof evohime.desktop.v1
                 * @interface IListChildHandoffs
                 * @augments evohime.desktop.v1.ListChildHandoffs.$Properties
                 * @deprecated Use evohime.desktop.v1.ListChildHandoffs.$Properties instead.
                 */

                /**
                 * Shape of a ListChildHandoffs.
                 * @typedef {evohime.desktop.v1.ListChildHandoffs.$Properties} evohime.desktop.v1.ListChildHandoffs.$Shape
                 */

                /**
                 * Constructs a new ListChildHandoffs.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ListChildHandoffs.
                 * @constructor
                 * @param {evohime.desktop.v1.ListChildHandoffs.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ListChildHandoffs = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ListChildHandoffs taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.ListChildHandoffs
                 * @instance
                 */
                ListChildHandoffs.prototype.taskId = "";

                /**
                 * ListChildHandoffs limit.
                 * @member {number} limit
                 * @memberof evohime.desktop.v1.ListChildHandoffs
                 * @instance
                 */
                ListChildHandoffs.prototype.limit = 0;

                /**
                 * Encodes the specified ListChildHandoffs message. Does not implicitly {@link evohime.desktop.v1.ListChildHandoffs.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ListChildHandoffs
                 * @static
                 * @param {evohime.desktop.v1.ListChildHandoffs.$Properties} message ListChildHandoffs message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ListChildHandoffs.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.limit != null && $Object.hasOwnProperty.call(message, "limit") && message.limit !== 0)
                        writer.uint32(/* id 2, wireType 0 =*/16).uint32(message.limit);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ListChildHandoffs message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ListChildHandoffs
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListChildHandoffs & evohime.desktop.v1.ListChildHandoffs.$Shape} ListChildHandoffs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ListChildHandoffs.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ListChildHandoffs(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.limit = value;
                                else
                                    delete message.limit;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ListChildHandoffs
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ListChildHandoffs
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ListChildHandoffs.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ListChildHandoffs";
                };

                return ListChildHandoffs;
            })();

            v1.SubmitChildRequest = (function() {

                /**
                 * Properties of a SubmitChildRequest.
                 * @typedef {Object} evohime.desktop.v1.SubmitChildRequest.$Properties
                 * @property {string|null} [childTaskId] SubmitChildRequest childTaskId
                 * @property {string|null} [parentTaskId] SubmitChildRequest parentTaskId
                 * @property {string|null} [role] SubmitChildRequest role
                 * @property {string|null} [kind] SubmitChildRequest kind
                 * @property {Array.<string>|null} [reducedContext] SubmitChildRequest reducedContext
                 * @property {number|null} [maxOutputBytes] SubmitChildRequest maxOutputBytes
                 * @property {Array.<string>|null} [requestedCapabilities] SubmitChildRequest requestedCapabilities
                 * @property {boolean|null} [parentIsChild] SubmitChildRequest parentIsChild
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a SubmitChildRequest.
                 * @memberof evohime.desktop.v1
                 * @interface ISubmitChildRequest
                 * @augments evohime.desktop.v1.SubmitChildRequest.$Properties
                 * @deprecated Use evohime.desktop.v1.SubmitChildRequest.$Properties instead.
                 */

                /**
                 * Shape of a SubmitChildRequest.
                 * @typedef {evohime.desktop.v1.SubmitChildRequest.$Properties} evohime.desktop.v1.SubmitChildRequest.$Shape
                 */

                /**
                 * Constructs a new SubmitChildRequest.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a SubmitChildRequest.
                 * @constructor
                 * @param {evohime.desktop.v1.SubmitChildRequest.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const SubmitChildRequest = function (properties) {
                    this.reducedContext = [];
                    this.requestedCapabilities = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * SubmitChildRequest childTaskId.
                 * @member {string} childTaskId
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.childTaskId = "";

                /**
                 * SubmitChildRequest parentTaskId.
                 * @member {string} parentTaskId
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.parentTaskId = "";

                /**
                 * SubmitChildRequest role.
                 * @member {string} role
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.role = "";

                /**
                 * SubmitChildRequest kind.
                 * @member {string} kind
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.kind = "";

                /**
                 * SubmitChildRequest reducedContext.
                 * @member {Array.<string>} reducedContext
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.reducedContext = $util.emptyArray;

                /**
                 * SubmitChildRequest maxOutputBytes.
                 * @member {number} maxOutputBytes
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.maxOutputBytes = 0;

                /**
                 * SubmitChildRequest requestedCapabilities.
                 * @member {Array.<string>} requestedCapabilities
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.requestedCapabilities = $util.emptyArray;

                /**
                 * SubmitChildRequest parentIsChild.
                 * @member {boolean} parentIsChild
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @instance
                 */
                SubmitChildRequest.prototype.parentIsChild = false;

                /**
                 * Encodes the specified SubmitChildRequest message. Does not implicitly {@link evohime.desktop.v1.SubmitChildRequest.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @static
                 * @param {evohime.desktop.v1.SubmitChildRequest.$Properties} message SubmitChildRequest message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                SubmitChildRequest.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.childTaskId != null && $Object.hasOwnProperty.call(message, "childTaskId") && message.childTaskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.childTaskId);
                    if (message.parentTaskId != null && $Object.hasOwnProperty.call(message, "parentTaskId") && message.parentTaskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.parentTaskId);
                    if (message.role != null && $Object.hasOwnProperty.call(message, "role") && message.role !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.role);
                    if (message.kind != null && $Object.hasOwnProperty.call(message, "kind") && message.kind !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.kind);
                    if (message.reducedContext != null && message.reducedContext.length)
                        for (let i = 0; i < message.reducedContext.length; ++i)
                            writer.uint32(/* id 5, wireType 2 =*/42).string(message.reducedContext[i]);
                    if (message.maxOutputBytes != null && $Object.hasOwnProperty.call(message, "maxOutputBytes") && message.maxOutputBytes !== 0)
                        writer.uint32(/* id 6, wireType 0 =*/48).uint32(message.maxOutputBytes);
                    if (message.requestedCapabilities != null && message.requestedCapabilities.length)
                        for (let i = 0; i < message.requestedCapabilities.length; ++i)
                            writer.uint32(/* id 7, wireType 2 =*/58).string(message.requestedCapabilities[i]);
                    if (message.parentIsChild != null && $Object.hasOwnProperty.call(message, "parentIsChild") && message.parentIsChild !== false)
                        writer.uint32(/* id 8, wireType 0 =*/64).bool(message.parentIsChild);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a SubmitChildRequest message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SubmitChildRequest & evohime.desktop.v1.SubmitChildRequest.$Shape} SubmitChildRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                SubmitChildRequest.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.SubmitChildRequest(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.childTaskId = value;
                                else
                                    delete message.childTaskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.parentTaskId = value;
                                else
                                    delete message.parentTaskId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.role = value;
                                else
                                    delete message.role;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.kind = value;
                                else
                                    delete message.kind;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.reducedContext && message.reducedContext.length))
                                    message.reducedContext = [];
                                message.reducedContext.push(reader.stringVerify());
                                continue;
                            }
                        case 6: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.maxOutputBytes = value;
                                else
                                    delete message.maxOutputBytes;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.requestedCapabilities && message.requestedCapabilities.length))
                                    message.requestedCapabilities = [];
                                message.requestedCapabilities.push(reader.stringVerify());
                                continue;
                            }
                        case 8: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.bool())
                                    message.parentIsChild = value;
                                else
                                    delete message.parentIsChild;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for SubmitChildRequest
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.SubmitChildRequest
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                SubmitChildRequest.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.SubmitChildRequest";
                };

                return SubmitChildRequest;
            })();

            v1.SubmitChildReport = (function() {

                /**
                 * Properties of a SubmitChildReport.
                 * @typedef {Object} evohime.desktop.v1.SubmitChildReport.$Properties
                 * @property {string|null} [childTaskId] SubmitChildReport childTaskId
                 * @property {string|null} [status] SubmitChildReport status
                 * @property {string|null} [summary] SubmitChildReport summary
                 * @property {Array.<string>|null} [findings] SubmitChildReport findings
                 * @property {Array.<string>|null} [sources] SubmitChildReport sources
                 * @property {number|null} [confidencePercent] SubmitChildReport confidencePercent
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a SubmitChildReport.
                 * @memberof evohime.desktop.v1
                 * @interface ISubmitChildReport
                 * @augments evohime.desktop.v1.SubmitChildReport.$Properties
                 * @deprecated Use evohime.desktop.v1.SubmitChildReport.$Properties instead.
                 */

                /**
                 * Shape of a SubmitChildReport.
                 * @typedef {evohime.desktop.v1.SubmitChildReport.$Properties} evohime.desktop.v1.SubmitChildReport.$Shape
                 */

                /**
                 * Constructs a new SubmitChildReport.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a SubmitChildReport.
                 * @constructor
                 * @param {evohime.desktop.v1.SubmitChildReport.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const SubmitChildReport = function (properties) {
                    this.findings = [];
                    this.sources = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * SubmitChildReport childTaskId.
                 * @member {string} childTaskId
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @instance
                 */
                SubmitChildReport.prototype.childTaskId = "";

                /**
                 * SubmitChildReport status.
                 * @member {string} status
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @instance
                 */
                SubmitChildReport.prototype.status = "";

                /**
                 * SubmitChildReport summary.
                 * @member {string} summary
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @instance
                 */
                SubmitChildReport.prototype.summary = "";

                /**
                 * SubmitChildReport findings.
                 * @member {Array.<string>} findings
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @instance
                 */
                SubmitChildReport.prototype.findings = $util.emptyArray;

                /**
                 * SubmitChildReport sources.
                 * @member {Array.<string>} sources
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @instance
                 */
                SubmitChildReport.prototype.sources = $util.emptyArray;

                /**
                 * SubmitChildReport confidencePercent.
                 * @member {number} confidencePercent
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @instance
                 */
                SubmitChildReport.prototype.confidencePercent = 0;

                /**
                 * Encodes the specified SubmitChildReport message. Does not implicitly {@link evohime.desktop.v1.SubmitChildReport.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @static
                 * @param {evohime.desktop.v1.SubmitChildReport.$Properties} message SubmitChildReport message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                SubmitChildReport.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.childTaskId != null && $Object.hasOwnProperty.call(message, "childTaskId") && message.childTaskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.childTaskId);
                    if (message.status != null && $Object.hasOwnProperty.call(message, "status") && message.status !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.status);
                    if (message.summary != null && $Object.hasOwnProperty.call(message, "summary") && message.summary !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.summary);
                    if (message.findings != null && message.findings.length)
                        for (let i = 0; i < message.findings.length; ++i)
                            writer.uint32(/* id 4, wireType 2 =*/34).string(message.findings[i]);
                    if (message.sources != null && message.sources.length)
                        for (let i = 0; i < message.sources.length; ++i)
                            writer.uint32(/* id 5, wireType 2 =*/42).string(message.sources[i]);
                    if (message.confidencePercent != null && $Object.hasOwnProperty.call(message, "confidencePercent") && message.confidencePercent !== 0)
                        writer.uint32(/* id 6, wireType 0 =*/48).uint32(message.confidencePercent);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a SubmitChildReport message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SubmitChildReport & evohime.desktop.v1.SubmitChildReport.$Shape} SubmitChildReport
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                SubmitChildReport.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.SubmitChildReport(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.childTaskId = value;
                                else
                                    delete message.childTaskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.status = value;
                                else
                                    delete message.status;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.summary = value;
                                else
                                    delete message.summary;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.findings && message.findings.length))
                                    message.findings = [];
                                message.findings.push(reader.stringVerify());
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.sources && message.sources.length))
                                    message.sources = [];
                                message.sources.push(reader.stringVerify());
                                continue;
                            }
                        case 6: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.confidencePercent = value;
                                else
                                    delete message.confidencePercent;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for SubmitChildReport
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.SubmitChildReport
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                SubmitChildReport.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.SubmitChildReport";
                };

                return SubmitChildReport;
            })();

            v1.GetCapabilitySelection = (function() {

                /**
                 * Properties of a GetCapabilitySelection.
                 * @typedef {Object} evohime.desktop.v1.GetCapabilitySelection.$Properties
                 * @property {string|null} [taskId] GetCapabilitySelection taskId
                 * @property {string|null} [intent] GetCapabilitySelection intent
                 * @property {Array.<string>|null} [requiredTools] GetCapabilitySelection requiredTools
                 * @property {Array.<string>|null} [requiredDomains] GetCapabilitySelection requiredDomains
                 * @property {string|null} [requestedRisk] GetCapabilitySelection requestedRisk
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a GetCapabilitySelection.
                 * @memberof evohime.desktop.v1
                 * @interface IGetCapabilitySelection
                 * @augments evohime.desktop.v1.GetCapabilitySelection.$Properties
                 * @deprecated Use evohime.desktop.v1.GetCapabilitySelection.$Properties instead.
                 */

                /**
                 * Shape of a GetCapabilitySelection.
                 * @typedef {evohime.desktop.v1.GetCapabilitySelection.$Properties} evohime.desktop.v1.GetCapabilitySelection.$Shape
                 */

                /**
                 * Constructs a new GetCapabilitySelection.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a GetCapabilitySelection.
                 * @constructor
                 * @param {evohime.desktop.v1.GetCapabilitySelection.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const GetCapabilitySelection = function (properties) {
                    this.requiredTools = [];
                    this.requiredDomains = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * GetCapabilitySelection taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @instance
                 */
                GetCapabilitySelection.prototype.taskId = "";

                /**
                 * GetCapabilitySelection intent.
                 * @member {string} intent
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @instance
                 */
                GetCapabilitySelection.prototype.intent = "";

                /**
                 * GetCapabilitySelection requiredTools.
                 * @member {Array.<string>} requiredTools
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @instance
                 */
                GetCapabilitySelection.prototype.requiredTools = $util.emptyArray;

                /**
                 * GetCapabilitySelection requiredDomains.
                 * @member {Array.<string>} requiredDomains
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @instance
                 */
                GetCapabilitySelection.prototype.requiredDomains = $util.emptyArray;

                /**
                 * GetCapabilitySelection requestedRisk.
                 * @member {string} requestedRisk
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @instance
                 */
                GetCapabilitySelection.prototype.requestedRisk = "";

                /**
                 * Encodes the specified GetCapabilitySelection message. Does not implicitly {@link evohime.desktop.v1.GetCapabilitySelection.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @static
                 * @param {evohime.desktop.v1.GetCapabilitySelection.$Properties} message GetCapabilitySelection message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                GetCapabilitySelection.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.intent != null && $Object.hasOwnProperty.call(message, "intent") && message.intent !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.intent);
                    if (message.requiredTools != null && message.requiredTools.length)
                        for (let i = 0; i < message.requiredTools.length; ++i)
                            writer.uint32(/* id 3, wireType 2 =*/26).string(message.requiredTools[i]);
                    if (message.requiredDomains != null && message.requiredDomains.length)
                        for (let i = 0; i < message.requiredDomains.length; ++i)
                            writer.uint32(/* id 4, wireType 2 =*/34).string(message.requiredDomains[i]);
                    if (message.requestedRisk != null && $Object.hasOwnProperty.call(message, "requestedRisk") && message.requestedRisk !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.requestedRisk);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a GetCapabilitySelection message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetCapabilitySelection & evohime.desktop.v1.GetCapabilitySelection.$Shape} GetCapabilitySelection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                GetCapabilitySelection.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.GetCapabilitySelection(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.intent = value;
                                else
                                    delete message.intent;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.requiredTools && message.requiredTools.length))
                                    message.requiredTools = [];
                                message.requiredTools.push(reader.stringVerify());
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.requiredDomains && message.requiredDomains.length))
                                    message.requiredDomains = [];
                                message.requiredDomains.push(reader.stringVerify());
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.requestedRisk = value;
                                else
                                    delete message.requestedRisk;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for GetCapabilitySelection
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.GetCapabilitySelection
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                GetCapabilitySelection.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.GetCapabilitySelection";
                };

                return GetCapabilitySelection;
            })();

            v1.PinCapabilitySelection = (function() {

                /**
                 * Properties of a PinCapabilitySelection.
                 * @typedef {Object} evohime.desktop.v1.PinCapabilitySelection.$Properties
                 * @property {string|null} [taskId] PinCapabilitySelection taskId
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a PinCapabilitySelection.
                 * @memberof evohime.desktop.v1
                 * @interface IPinCapabilitySelection
                 * @augments evohime.desktop.v1.PinCapabilitySelection.$Properties
                 * @deprecated Use evohime.desktop.v1.PinCapabilitySelection.$Properties instead.
                 */

                /**
                 * Shape of a PinCapabilitySelection.
                 * @typedef {evohime.desktop.v1.PinCapabilitySelection.$Properties} evohime.desktop.v1.PinCapabilitySelection.$Shape
                 */

                /**
                 * Constructs a new PinCapabilitySelection.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a PinCapabilitySelection.
                 * @constructor
                 * @param {evohime.desktop.v1.PinCapabilitySelection.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const PinCapabilitySelection = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * PinCapabilitySelection taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.PinCapabilitySelection
                 * @instance
                 */
                PinCapabilitySelection.prototype.taskId = "";

                /**
                 * Encodes the specified PinCapabilitySelection message. Does not implicitly {@link evohime.desktop.v1.PinCapabilitySelection.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.PinCapabilitySelection
                 * @static
                 * @param {evohime.desktop.v1.PinCapabilitySelection.$Properties} message PinCapabilitySelection message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                PinCapabilitySelection.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a PinCapabilitySelection message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.PinCapabilitySelection
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PinCapabilitySelection & evohime.desktop.v1.PinCapabilitySelection.$Shape} PinCapabilitySelection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                PinCapabilitySelection.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.PinCapabilitySelection(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for PinCapabilitySelection
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.PinCapabilitySelection
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                PinCapabilitySelection.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.PinCapabilitySelection";
                };

                return PinCapabilitySelection;
            })();

            v1.ReplaceCapabilitySelection = (function() {

                /**
                 * Properties of a ReplaceCapabilitySelection.
                 * @typedef {Object} evohime.desktop.v1.ReplaceCapabilitySelection.$Properties
                 * @property {string|null} [taskId] ReplaceCapabilitySelection taskId
                 * @property {string|null} [manifestName] ReplaceCapabilitySelection manifestName
                 * @property {string|null} [intent] ReplaceCapabilitySelection intent
                 * @property {Array.<string>|null} [requiredTools] ReplaceCapabilitySelection requiredTools
                 * @property {Array.<string>|null} [requiredDomains] ReplaceCapabilitySelection requiredDomains
                 * @property {string|null} [requestedRisk] ReplaceCapabilitySelection requestedRisk
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ReplaceCapabilitySelection.
                 * @memberof evohime.desktop.v1
                 * @interface IReplaceCapabilitySelection
                 * @augments evohime.desktop.v1.ReplaceCapabilitySelection.$Properties
                 * @deprecated Use evohime.desktop.v1.ReplaceCapabilitySelection.$Properties instead.
                 */

                /**
                 * Shape of a ReplaceCapabilitySelection.
                 * @typedef {evohime.desktop.v1.ReplaceCapabilitySelection.$Properties} evohime.desktop.v1.ReplaceCapabilitySelection.$Shape
                 */

                /**
                 * Constructs a new ReplaceCapabilitySelection.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ReplaceCapabilitySelection.
                 * @constructor
                 * @param {evohime.desktop.v1.ReplaceCapabilitySelection.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ReplaceCapabilitySelection = function (properties) {
                    this.requiredTools = [];
                    this.requiredDomains = [];
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ReplaceCapabilitySelection taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @instance
                 */
                ReplaceCapabilitySelection.prototype.taskId = "";

                /**
                 * ReplaceCapabilitySelection manifestName.
                 * @member {string} manifestName
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @instance
                 */
                ReplaceCapabilitySelection.prototype.manifestName = "";

                /**
                 * ReplaceCapabilitySelection intent.
                 * @member {string} intent
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @instance
                 */
                ReplaceCapabilitySelection.prototype.intent = "";

                /**
                 * ReplaceCapabilitySelection requiredTools.
                 * @member {Array.<string>} requiredTools
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @instance
                 */
                ReplaceCapabilitySelection.prototype.requiredTools = $util.emptyArray;

                /**
                 * ReplaceCapabilitySelection requiredDomains.
                 * @member {Array.<string>} requiredDomains
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @instance
                 */
                ReplaceCapabilitySelection.prototype.requiredDomains = $util.emptyArray;

                /**
                 * ReplaceCapabilitySelection requestedRisk.
                 * @member {string} requestedRisk
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @instance
                 */
                ReplaceCapabilitySelection.prototype.requestedRisk = "";

                /**
                 * Encodes the specified ReplaceCapabilitySelection message. Does not implicitly {@link evohime.desktop.v1.ReplaceCapabilitySelection.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @static
                 * @param {evohime.desktop.v1.ReplaceCapabilitySelection.$Properties} message ReplaceCapabilitySelection message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ReplaceCapabilitySelection.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.taskId);
                    if (message.manifestName != null && $Object.hasOwnProperty.call(message, "manifestName") && message.manifestName !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.manifestName);
                    if (message.intent != null && $Object.hasOwnProperty.call(message, "intent") && message.intent !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.intent);
                    if (message.requiredTools != null && message.requiredTools.length)
                        for (let i = 0; i < message.requiredTools.length; ++i)
                            writer.uint32(/* id 4, wireType 2 =*/34).string(message.requiredTools[i]);
                    if (message.requiredDomains != null && message.requiredDomains.length)
                        for (let i = 0; i < message.requiredDomains.length; ++i)
                            writer.uint32(/* id 5, wireType 2 =*/42).string(message.requiredDomains[i]);
                    if (message.requestedRisk != null && $Object.hasOwnProperty.call(message, "requestedRisk") && message.requestedRisk !== "")
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.requestedRisk);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ReplaceCapabilitySelection message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReplaceCapabilitySelection & evohime.desktop.v1.ReplaceCapabilitySelection.$Shape} ReplaceCapabilitySelection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ReplaceCapabilitySelection.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ReplaceCapabilitySelection(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.manifestName = value;
                                else
                                    delete message.manifestName;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.intent = value;
                                else
                                    delete message.intent;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.requiredTools && message.requiredTools.length))
                                    message.requiredTools = [];
                                message.requiredTools.push(reader.stringVerify());
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if (!(message.requiredDomains && message.requiredDomains.length))
                                    message.requiredDomains = [];
                                message.requiredDomains.push(reader.stringVerify());
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.requestedRisk = value;
                                else
                                    delete message.requestedRisk;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ReplaceCapabilitySelection
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ReplaceCapabilitySelection
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ReplaceCapabilitySelection.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ReplaceCapabilitySelection";
                };

                return ReplaceCapabilitySelection;
            })();

            v1.SubmitFeedback = (function() {

                /**
                 * Properties of a SubmitFeedback.
                 * @typedef {Object} evohime.desktop.v1.SubmitFeedback.$Properties
                 * @property {string|null} [runId] SubmitFeedback runId
                 * @property {string|null} [taskId] SubmitFeedback taskId
                 * @property {string|null} [subjectRef] SubmitFeedback subjectRef
                 * @property {string|null} [signal] SubmitFeedback signal
                 * @property {string|null} [correction] SubmitFeedback correction
                 * @property {string|null} [rejectionReason] SubmitFeedback rejectionReason
                 * @property {string|null} [outcome] SubmitFeedback outcome
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a SubmitFeedback.
                 * @memberof evohime.desktop.v1
                 * @interface ISubmitFeedback
                 * @augments evohime.desktop.v1.SubmitFeedback.$Properties
                 * @deprecated Use evohime.desktop.v1.SubmitFeedback.$Properties instead.
                 */

                /**
                 * Shape of a SubmitFeedback.
                 * @typedef {evohime.desktop.v1.SubmitFeedback.$Properties} evohime.desktop.v1.SubmitFeedback.$Shape
                 */

                /**
                 * Constructs a new SubmitFeedback.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a SubmitFeedback.
                 * @constructor
                 * @param {evohime.desktop.v1.SubmitFeedback.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const SubmitFeedback = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * SubmitFeedback runId.
                 * @member {string} runId
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @instance
                 */
                SubmitFeedback.prototype.runId = "";

                /**
                 * SubmitFeedback taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @instance
                 */
                SubmitFeedback.prototype.taskId = "";

                /**
                 * SubmitFeedback subjectRef.
                 * @member {string} subjectRef
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @instance
                 */
                SubmitFeedback.prototype.subjectRef = "";

                /**
                 * SubmitFeedback signal.
                 * @member {string} signal
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @instance
                 */
                SubmitFeedback.prototype.signal = "";

                /**
                 * SubmitFeedback correction.
                 * @member {string} correction
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @instance
                 */
                SubmitFeedback.prototype.correction = "";

                /**
                 * SubmitFeedback rejectionReason.
                 * @member {string} rejectionReason
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @instance
                 */
                SubmitFeedback.prototype.rejectionReason = "";

                /**
                 * SubmitFeedback outcome.
                 * @member {string} outcome
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @instance
                 */
                SubmitFeedback.prototype.outcome = "";

                /**
                 * Encodes the specified SubmitFeedback message. Does not implicitly {@link evohime.desktop.v1.SubmitFeedback.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @static
                 * @param {evohime.desktop.v1.SubmitFeedback.$Properties} message SubmitFeedback message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                SubmitFeedback.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.runId != null && $Object.hasOwnProperty.call(message, "runId") && message.runId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.runId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.taskId);
                    if (message.subjectRef != null && $Object.hasOwnProperty.call(message, "subjectRef") && message.subjectRef !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.subjectRef);
                    if (message.signal != null && $Object.hasOwnProperty.call(message, "signal") && message.signal !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.signal);
                    if (message.correction != null && $Object.hasOwnProperty.call(message, "correction") && message.correction !== "")
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.correction);
                    if (message.rejectionReason != null && $Object.hasOwnProperty.call(message, "rejectionReason") && message.rejectionReason !== "")
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.rejectionReason);
                    if (message.outcome != null && $Object.hasOwnProperty.call(message, "outcome") && message.outcome !== "")
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.outcome);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a SubmitFeedback message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SubmitFeedback & evohime.desktop.v1.SubmitFeedback.$Shape} SubmitFeedback
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                SubmitFeedback.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.SubmitFeedback(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.runId = value;
                                else
                                    delete message.runId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.subjectRef = value;
                                else
                                    delete message.subjectRef;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.signal = value;
                                else
                                    delete message.signal;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.correction = value;
                                else
                                    delete message.correction;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.rejectionReason = value;
                                else
                                    delete message.rejectionReason;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.outcome = value;
                                else
                                    delete message.outcome;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for SubmitFeedback
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.SubmitFeedback
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                SubmitFeedback.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.SubmitFeedback";
                };

                return SubmitFeedback;
            })();

            v1.ListFeedback = (function() {

                /**
                 * Properties of a ListFeedback.
                 * @typedef {Object} evohime.desktop.v1.ListFeedback.$Properties
                 * @property {string|null} [runId] ListFeedback runId
                 * @property {number|null} [limit] ListFeedback limit
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ListFeedback.
                 * @memberof evohime.desktop.v1
                 * @interface IListFeedback
                 * @augments evohime.desktop.v1.ListFeedback.$Properties
                 * @deprecated Use evohime.desktop.v1.ListFeedback.$Properties instead.
                 */

                /**
                 * Shape of a ListFeedback.
                 * @typedef {evohime.desktop.v1.ListFeedback.$Properties} evohime.desktop.v1.ListFeedback.$Shape
                 */

                /**
                 * Constructs a new ListFeedback.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ListFeedback.
                 * @constructor
                 * @param {evohime.desktop.v1.ListFeedback.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ListFeedback = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ListFeedback runId.
                 * @member {string} runId
                 * @memberof evohime.desktop.v1.ListFeedback
                 * @instance
                 */
                ListFeedback.prototype.runId = "";

                /**
                 * ListFeedback limit.
                 * @member {number} limit
                 * @memberof evohime.desktop.v1.ListFeedback
                 * @instance
                 */
                ListFeedback.prototype.limit = 0;

                /**
                 * Encodes the specified ListFeedback message. Does not implicitly {@link evohime.desktop.v1.ListFeedback.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ListFeedback
                 * @static
                 * @param {evohime.desktop.v1.ListFeedback.$Properties} message ListFeedback message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ListFeedback.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.runId != null && $Object.hasOwnProperty.call(message, "runId") && message.runId !== "")
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.runId);
                    if (message.limit != null && $Object.hasOwnProperty.call(message, "limit") && message.limit !== 0)
                        writer.uint32(/* id 2, wireType 0 =*/16).uint32(message.limit);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ListFeedback message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ListFeedback
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListFeedback & evohime.desktop.v1.ListFeedback.$Shape} ListFeedback
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ListFeedback.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ListFeedback(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.runId = value;
                                else
                                    delete message.runId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (value = reader.uint32())
                                    message.limit = value;
                                else
                                    delete message.limit;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ListFeedback
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ListFeedback
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ListFeedback.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ListFeedback";
                };

                return ListFeedback;
            })();

            v1.CommandEnvelope = (function() {

                /**
                 * Properties of a CommandEnvelope.
                 * @typedef {Object} evohime.desktop.v1.CommandEnvelope.$Properties
                 * @property {evohime.desktop.v1.ProtocolVersion.$Properties|null} [protocol] CommandEnvelope protocol
                 * @property {string|null} [requestId] CommandEnvelope requestId
                 * @property {string|null} [clientId] CommandEnvelope clientId
                 * @property {string|null} [coreInstanceId] CommandEnvelope coreInstanceId
                 * @property {number|null} [sessionEpoch] CommandEnvelope sessionEpoch
                 * @property {evohime.desktop.v1.Handshake.$Properties|null} [handshake] CommandEnvelope handshake
                 * @property {evohime.desktop.v1.ReplayEvents.$Properties|null} [replayEvents] CommandEnvelope replayEvents
                 * @property {evohime.desktop.v1.StartTask.$Properties|null} [startTask] CommandEnvelope startTask
                 * @property {evohime.desktop.v1.StopTask.$Properties|null} [stopTask] CommandEnvelope stopTask
                 * @property {evohime.desktop.v1.ResolveApproval.$Properties|null} [resolveApproval] CommandEnvelope resolveApproval
                 * @property {evohime.desktop.v1.ModelConfigRequest.$Properties|null} [modelConfig] CommandEnvelope modelConfig
                 * @property {evohime.desktop.v1.ModelCatalogRequest.$Properties|null} [modelCatalog] CommandEnvelope modelCatalog
                 * @property {evohime.desktop.v1.PermissionModeRequest.$Properties|null} [permissionMode] CommandEnvelope permissionMode
                 * @property {evohime.desktop.v1.CreateProject.$Properties|null} [createProject] CommandEnvelope createProject
                 * @property {evohime.desktop.v1.CreateTask.$Properties|null} [createTask] CommandEnvelope createTask
                 * @property {evohime.desktop.v1.UpdateTaskStatus.$Properties|null} [updateTaskStatus] CommandEnvelope updateTaskStatus
                 * @property {evohime.desktop.v1.AddTaskEdge.$Properties|null} [addTaskEdge] CommandEnvelope addTaskEdge
                 * @property {evohime.desktop.v1.GetTaskGraph.$Properties|null} [getTaskGraph] CommandEnvelope getTaskGraph
                 * @property {evohime.desktop.v1.NextReadyTask.$Properties|null} [nextReadyTask] CommandEnvelope nextReadyTask
                 * @property {evohime.desktop.v1.ImportPrd.$Properties|null} [importPrd] CommandEnvelope importPrd
                 * @property {evohime.desktop.v1.GetTaskHistory.$Properties|null} [getTaskHistory] CommandEnvelope getTaskHistory
                 * @property {evohime.desktop.v1.GetTaskContext.$Properties|null} [getTaskContext] CommandEnvelope getTaskContext
                 * @property {evohime.desktop.v1.GetTaskPlanSpec.$Properties|null} [getTaskPlanSpec] CommandEnvelope getTaskPlanSpec
                 * @property {evohime.desktop.v1.ApplyApprovedBuild.$Properties|null} [applyApprovedBuild] CommandEnvelope applyApprovedBuild
                 * @property {evohime.desktop.v1.PrepareBuild.$Properties|null} [prepareBuild] CommandEnvelope prepareBuild
                 * @property {evohime.desktop.v1.GetTaskSnapshot.$Properties|null} [getTaskSnapshot] CommandEnvelope getTaskSnapshot
                 * @property {evohime.desktop.v1.RestoreTaskSnapshot.$Properties|null} [restoreTaskSnapshot] CommandEnvelope restoreTaskSnapshot
                 * @property {evohime.desktop.v1.GetBuildPolicy.$Properties|null} [getBuildPolicy] CommandEnvelope getBuildPolicy
                 * @property {evohime.desktop.v1.SaveBuildPolicy.$Properties|null} [saveBuildPolicy] CommandEnvelope saveBuildPolicy
                 * @property {evohime.desktop.v1.ResyncRequest.$Properties|null} [resyncRequest] CommandEnvelope resyncRequest
                 * @property {evohime.desktop.v1.RunDoctor.$Properties|null} [runDoctor] CommandEnvelope runDoctor
                 * @property {evohime.desktop.v1.SaveResearchEvidence.$Properties|null} [saveResearchEvidence] CommandEnvelope saveResearchEvidence
                 * @property {evohime.desktop.v1.ListResearchEvidence.$Properties|null} [listResearchEvidence] CommandEnvelope listResearchEvidence
                 * @property {evohime.desktop.v1.CreateMemory.$Properties|null} [createMemory] CommandEnvelope createMemory
                 * @property {evohime.desktop.v1.ListMemory.$Properties|null} [listMemory] CommandEnvelope listMemory
                 * @property {evohime.desktop.v1.SearchMemory.$Properties|null} [searchMemory] CommandEnvelope searchMemory
                 * @property {evohime.desktop.v1.ArchiveMemory.$Properties|null} [archiveMemory] CommandEnvelope archiveMemory
                 * @property {evohime.desktop.v1.ForgetMemory.$Properties|null} [forgetMemory] CommandEnvelope forgetMemory
                 * @property {evohime.desktop.v1.InstallCapability.$Properties|null} [installCapability] CommandEnvelope installCapability
                 * @property {evohime.desktop.v1.ListCapabilities.$Properties|null} [listCapabilities] CommandEnvelope listCapabilities
                 * @property {evohime.desktop.v1.MatchCapabilities.$Properties|null} [matchCapabilities] CommandEnvelope matchCapabilities
                 * @property {evohime.desktop.v1.RemoveCapability.$Properties|null} [removeCapability] CommandEnvelope removeCapability
                 * @property {evohime.desktop.v1.RequestChildHandoff.$Properties|null} [requestChildHandoff] CommandEnvelope requestChildHandoff
                 * @property {evohime.desktop.v1.ListChildHandoffs.$Properties|null} [listChildHandoffs] CommandEnvelope listChildHandoffs
                 * @property {evohime.desktop.v1.SubmitChildRequest.$Properties|null} [submitChildRequest] CommandEnvelope submitChildRequest
                 * @property {evohime.desktop.v1.SubmitChildReport.$Properties|null} [submitChildReport] CommandEnvelope submitChildReport
                 * @property {evohime.desktop.v1.RunResearchFetch.$Properties|null} [runResearchFetch] CommandEnvelope runResearchFetch
                 * @property {evohime.desktop.v1.ListWorkspace.$Properties|null} [listWorkspace] CommandEnvelope listWorkspace
                 * @property {evohime.desktop.v1.ReadWorkspaceFile.$Properties|null} [readWorkspaceFile] CommandEnvelope readWorkspaceFile
                 * @property {evohime.desktop.v1.GitStatus.$Properties|null} [gitStatus] CommandEnvelope gitStatus
                 * @property {evohime.desktop.v1.GitDiff.$Properties|null} [gitDiff] CommandEnvelope gitDiff
                 * @property {evohime.desktop.v1.TerminalExecute.$Properties|null} [terminalExecute] CommandEnvelope terminalExecute
                 * @property {evohime.desktop.v1.ExportDoctorLogs.$Properties|null} [exportDoctorLogs] CommandEnvelope exportDoctorLogs
                 * @property {evohime.desktop.v1.GetCapabilitySelection.$Properties|null} [getCapabilitySelection] CommandEnvelope getCapabilitySelection
                 * @property {evohime.desktop.v1.PinCapabilitySelection.$Properties|null} [pinCapabilitySelection] CommandEnvelope pinCapabilitySelection
                 * @property {evohime.desktop.v1.ReplaceCapabilitySelection.$Properties|null} [replaceCapabilitySelection] CommandEnvelope replaceCapabilitySelection
                 * @property {evohime.desktop.v1.SubmitFeedback.$Properties|null} [submitFeedback] CommandEnvelope submitFeedback
                 * @property {evohime.desktop.v1.ListFeedback.$Properties|null} [listFeedback] CommandEnvelope listFeedback
                 * @property {evohime.desktop.v1.CreateDatabaseBackup.$Properties|null} [createDatabaseBackup] CommandEnvelope createDatabaseBackup
                 * @property {evohime.desktop.v1.PrepareDatabaseRestore.$Properties|null} [prepareDatabaseRestore] CommandEnvelope prepareDatabaseRestore
                 * @property {evohime.desktop.v1.RestoreDatabase.$Properties|null} [restoreDatabase] CommandEnvelope restoreDatabase
                 * @property {evohime.desktop.v1.SelectModelRequest.$Properties|null} [selectModel] CommandEnvelope selectModel
                 * @property {evohime.desktop.v1.CancelDatabaseOperation.$Properties|null} [cancelDatabaseOperation] CommandEnvelope cancelDatabaseOperation
                 * @property {"handshake"|"replayEvents"|"startTask"|"stopTask"|"resolveApproval"|"modelConfig"|"modelCatalog"|"permissionMode"|"createProject"|"createTask"|"updateTaskStatus"|"addTaskEdge"|"getTaskGraph"|"nextReadyTask"|"importPrd"|"getTaskHistory"|"getTaskContext"|"getTaskPlanSpec"|"applyApprovedBuild"|"prepareBuild"|"getTaskSnapshot"|"restoreTaskSnapshot"|"getBuildPolicy"|"saveBuildPolicy"|"resyncRequest"|"runDoctor"|"saveResearchEvidence"|"listResearchEvidence"|"createMemory"|"listMemory"|"searchMemory"|"archiveMemory"|"forgetMemory"|"installCapability"|"listCapabilities"|"matchCapabilities"|"removeCapability"|"requestChildHandoff"|"listChildHandoffs"|"submitChildRequest"|"submitChildReport"|"runResearchFetch"|"listWorkspace"|"readWorkspaceFile"|"gitStatus"|"gitDiff"|"terminalExecute"|"exportDoctorLogs"|"getCapabilitySelection"|"pinCapabilitySelection"|"replaceCapabilitySelection"|"submitFeedback"|"listFeedback"|"createDatabaseBackup"|"prepareDatabaseRestore"|"restoreDatabase"|"selectModel"|"cancelDatabaseOperation"} [command] CommandEnvelope command
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a CommandEnvelope.
                 * @memberof evohime.desktop.v1
                 * @interface ICommandEnvelope
                 * @augments evohime.desktop.v1.CommandEnvelope.$Properties
                 * @deprecated Use evohime.desktop.v1.CommandEnvelope.$Properties instead.
                 */

                /**
                 * Narrowed shape of a CommandEnvelope.
                 * @typedef {{
                 *   protocol?: evohime.desktop.v1.ProtocolVersion.$Shape|null;
                 *   requestId?: string|null;
                 *   clientId?: string|null;
                 *   coreInstanceId?: string|null;
                 *   sessionEpoch?: number|null;
                 *   handshake?: evohime.desktop.v1.Handshake.$Shape|null;
                 *   replayEvents?: evohime.desktop.v1.ReplayEvents.$Shape|null;
                 *   startTask?: evohime.desktop.v1.StartTask.$Shape|null;
                 *   stopTask?: evohime.desktop.v1.StopTask.$Shape|null;
                 *   resolveApproval?: evohime.desktop.v1.ResolveApproval.$Shape|null;
                 *   modelConfig?: evohime.desktop.v1.ModelConfigRequest.$Shape|null;
                 *   modelCatalog?: evohime.desktop.v1.ModelCatalogRequest.$Shape|null;
                 *   permissionMode?: evohime.desktop.v1.PermissionModeRequest.$Shape|null;
                 *   createProject?: evohime.desktop.v1.CreateProject.$Shape|null;
                 *   createTask?: evohime.desktop.v1.CreateTask.$Shape|null;
                 *   updateTaskStatus?: evohime.desktop.v1.UpdateTaskStatus.$Shape|null;
                 *   addTaskEdge?: evohime.desktop.v1.AddTaskEdge.$Shape|null;
                 *   getTaskGraph?: evohime.desktop.v1.GetTaskGraph.$Shape|null;
                 *   nextReadyTask?: evohime.desktop.v1.NextReadyTask.$Shape|null;
                 *   importPrd?: evohime.desktop.v1.ImportPrd.$Shape|null;
                 *   getTaskHistory?: evohime.desktop.v1.GetTaskHistory.$Shape|null;
                 *   getTaskContext?: evohime.desktop.v1.GetTaskContext.$Shape|null;
                 *   getTaskPlanSpec?: evohime.desktop.v1.GetTaskPlanSpec.$Shape|null;
                 *   applyApprovedBuild?: evohime.desktop.v1.ApplyApprovedBuild.$Shape|null;
                 *   prepareBuild?: evohime.desktop.v1.PrepareBuild.$Shape|null;
                 *   getTaskSnapshot?: evohime.desktop.v1.GetTaskSnapshot.$Shape|null;
                 *   restoreTaskSnapshot?: evohime.desktop.v1.RestoreTaskSnapshot.$Shape|null;
                 *   getBuildPolicy?: evohime.desktop.v1.GetBuildPolicy.$Shape|null;
                 *   saveBuildPolicy?: evohime.desktop.v1.SaveBuildPolicy.$Shape|null;
                 *   resyncRequest?: evohime.desktop.v1.ResyncRequest.$Shape|null;
                 *   runDoctor?: evohime.desktop.v1.RunDoctor.$Shape|null;
                 *   saveResearchEvidence?: evohime.desktop.v1.SaveResearchEvidence.$Shape|null;
                 *   listResearchEvidence?: evohime.desktop.v1.ListResearchEvidence.$Shape|null;
                 *   createMemory?: evohime.desktop.v1.CreateMemory.$Shape|null;
                 *   listMemory?: evohime.desktop.v1.ListMemory.$Shape|null;
                 *   searchMemory?: evohime.desktop.v1.SearchMemory.$Shape|null;
                 *   archiveMemory?: evohime.desktop.v1.ArchiveMemory.$Shape|null;
                 *   forgetMemory?: evohime.desktop.v1.ForgetMemory.$Shape|null;
                 *   installCapability?: evohime.desktop.v1.InstallCapability.$Shape|null;
                 *   listCapabilities?: evohime.desktop.v1.ListCapabilities.$Shape|null;
                 *   matchCapabilities?: evohime.desktop.v1.MatchCapabilities.$Shape|null;
                 *   removeCapability?: evohime.desktop.v1.RemoveCapability.$Shape|null;
                 *   requestChildHandoff?: evohime.desktop.v1.RequestChildHandoff.$Shape|null;
                 *   listChildHandoffs?: evohime.desktop.v1.ListChildHandoffs.$Shape|null;
                 *   submitChildRequest?: evohime.desktop.v1.SubmitChildRequest.$Shape|null;
                 *   submitChildReport?: evohime.desktop.v1.SubmitChildReport.$Shape|null;
                 *   runResearchFetch?: evohime.desktop.v1.RunResearchFetch.$Shape|null;
                 *   listWorkspace?: evohime.desktop.v1.ListWorkspace.$Shape|null;
                 *   readWorkspaceFile?: evohime.desktop.v1.ReadWorkspaceFile.$Shape|null;
                 *   gitStatus?: evohime.desktop.v1.GitStatus.$Shape|null;
                 *   gitDiff?: evohime.desktop.v1.GitDiff.$Shape|null;
                 *   terminalExecute?: evohime.desktop.v1.TerminalExecute.$Shape|null;
                 *   exportDoctorLogs?: evohime.desktop.v1.ExportDoctorLogs.$Shape|null;
                 *   getCapabilitySelection?: evohime.desktop.v1.GetCapabilitySelection.$Shape|null;
                 *   pinCapabilitySelection?: evohime.desktop.v1.PinCapabilitySelection.$Shape|null;
                 *   replaceCapabilitySelection?: evohime.desktop.v1.ReplaceCapabilitySelection.$Shape|null;
                 *   submitFeedback?: evohime.desktop.v1.SubmitFeedback.$Shape|null;
                 *   listFeedback?: evohime.desktop.v1.ListFeedback.$Shape|null;
                 *   createDatabaseBackup?: evohime.desktop.v1.CreateDatabaseBackup.$Shape|null;
                 *   prepareDatabaseRestore?: evohime.desktop.v1.PrepareDatabaseRestore.$Shape|null;
                 *   restoreDatabase?: evohime.desktop.v1.RestoreDatabase.$Shape|null;
                 *   selectModel?: evohime.desktop.v1.SelectModelRequest.$Shape|null;
                 *   cancelDatabaseOperation?: evohime.desktop.v1.CancelDatabaseOperation.$Shape|null;
                 *   $unknowns?: Array.<Uint8Array>;
                 * } & (
                 *   ({ command?: undefined; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "handshake"; handshake: evohime.desktop.v1.Handshake.$Shape; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "replayEvents"; handshake?: null; replayEvents: evohime.desktop.v1.ReplayEvents.$Shape; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "startTask"; handshake?: null; replayEvents?: null; startTask: evohime.desktop.v1.StartTask.$Shape; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "stopTask"; handshake?: null; replayEvents?: null; startTask?: null; stopTask: evohime.desktop.v1.StopTask.$Shape; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "resolveApproval"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval: evohime.desktop.v1.ResolveApproval.$Shape; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "modelConfig"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig: evohime.desktop.v1.ModelConfigRequest.$Shape; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "modelCatalog"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog: evohime.desktop.v1.ModelCatalogRequest.$Shape; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "permissionMode"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode: evohime.desktop.v1.PermissionModeRequest.$Shape; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "createProject"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject: evohime.desktop.v1.CreateProject.$Shape; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "createTask"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask: evohime.desktop.v1.CreateTask.$Shape; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "updateTaskStatus"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus: evohime.desktop.v1.UpdateTaskStatus.$Shape; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "addTaskEdge"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge: evohime.desktop.v1.AddTaskEdge.$Shape; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "getTaskGraph"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph: evohime.desktop.v1.GetTaskGraph.$Shape; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "nextReadyTask"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask: evohime.desktop.v1.NextReadyTask.$Shape; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "importPrd"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd: evohime.desktop.v1.ImportPrd.$Shape; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "getTaskHistory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory: evohime.desktop.v1.GetTaskHistory.$Shape; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "getTaskContext"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext: evohime.desktop.v1.GetTaskContext.$Shape; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "getTaskPlanSpec"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec: evohime.desktop.v1.GetTaskPlanSpec.$Shape; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "applyApprovedBuild"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild: evohime.desktop.v1.ApplyApprovedBuild.$Shape; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "prepareBuild"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild: evohime.desktop.v1.PrepareBuild.$Shape; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "getTaskSnapshot"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot: evohime.desktop.v1.GetTaskSnapshot.$Shape; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "restoreTaskSnapshot"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot: evohime.desktop.v1.RestoreTaskSnapshot.$Shape; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "getBuildPolicy"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy: evohime.desktop.v1.GetBuildPolicy.$Shape; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "saveBuildPolicy"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy: evohime.desktop.v1.SaveBuildPolicy.$Shape; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "resyncRequest"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest: evohime.desktop.v1.ResyncRequest.$Shape; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "runDoctor"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor: evohime.desktop.v1.RunDoctor.$Shape; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "saveResearchEvidence"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence: evohime.desktop.v1.SaveResearchEvidence.$Shape; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "listResearchEvidence"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence: evohime.desktop.v1.ListResearchEvidence.$Shape; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "createMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory: evohime.desktop.v1.CreateMemory.$Shape; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "listMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory: evohime.desktop.v1.ListMemory.$Shape; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "searchMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory: evohime.desktop.v1.SearchMemory.$Shape; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "archiveMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory: evohime.desktop.v1.ArchiveMemory.$Shape; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "forgetMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory: evohime.desktop.v1.ForgetMemory.$Shape; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "installCapability"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability: evohime.desktop.v1.InstallCapability.$Shape; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "listCapabilities"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities: evohime.desktop.v1.ListCapabilities.$Shape; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "matchCapabilities"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities: evohime.desktop.v1.MatchCapabilities.$Shape; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "removeCapability"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability: evohime.desktop.v1.RemoveCapability.$Shape; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "requestChildHandoff"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff: evohime.desktop.v1.RequestChildHandoff.$Shape; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "listChildHandoffs"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs: evohime.desktop.v1.ListChildHandoffs.$Shape; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "submitChildRequest"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest: evohime.desktop.v1.SubmitChildRequest.$Shape; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "submitChildReport"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport: evohime.desktop.v1.SubmitChildReport.$Shape; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "runResearchFetch"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch: evohime.desktop.v1.RunResearchFetch.$Shape; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "listWorkspace"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace: evohime.desktop.v1.ListWorkspace.$Shape; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "readWorkspaceFile"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile: evohime.desktop.v1.ReadWorkspaceFile.$Shape; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "gitStatus"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus: evohime.desktop.v1.GitStatus.$Shape; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "gitDiff"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff: evohime.desktop.v1.GitDiff.$Shape; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "terminalExecute"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute: evohime.desktop.v1.TerminalExecute.$Shape; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "exportDoctorLogs"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs: evohime.desktop.v1.ExportDoctorLogs.$Shape; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "getCapabilitySelection"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection: evohime.desktop.v1.GetCapabilitySelection.$Shape; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "pinCapabilitySelection"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection: evohime.desktop.v1.PinCapabilitySelection.$Shape; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "replaceCapabilitySelection"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection: evohime.desktop.v1.ReplaceCapabilitySelection.$Shape; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "submitFeedback"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback: evohime.desktop.v1.SubmitFeedback.$Shape; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "listFeedback"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback: evohime.desktop.v1.ListFeedback.$Shape; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "createDatabaseBackup"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup: evohime.desktop.v1.CreateDatabaseBackup.$Shape; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "prepareDatabaseRestore"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore: evohime.desktop.v1.PrepareDatabaseRestore.$Shape; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "restoreDatabase"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase: evohime.desktop.v1.RestoreDatabase.$Shape; selectModel?: null; cancelDatabaseOperation?: null }|{ command?: "selectModel"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel: evohime.desktop.v1.SelectModelRequest.$Shape; cancelDatabaseOperation?: null }|{ command?: "cancelDatabaseOperation"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation: evohime.desktop.v1.CancelDatabaseOperation.$Shape })
                 * )} evohime.desktop.v1.CommandEnvelope.$Shape
                 */

                /**
                 * Constructs a new CommandEnvelope.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a CommandEnvelope.
                 * @constructor
                 * @param {evohime.desktop.v1.CommandEnvelope.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const CommandEnvelope = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * CommandEnvelope protocol.
                 * @member {evohime.desktop.v1.ProtocolVersion.$Properties|null|undefined} protocol
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.protocol = null;

                /**
                 * CommandEnvelope requestId.
                 * @member {string} requestId
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.requestId = "";

                /**
                 * CommandEnvelope clientId.
                 * @member {string} clientId
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.clientId = "";

                /**
                 * CommandEnvelope coreInstanceId.
                 * @member {string} coreInstanceId
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.coreInstanceId = "";

                /**
                 * CommandEnvelope sessionEpoch.
                 * @member {number} sessionEpoch
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.sessionEpoch = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * CommandEnvelope handshake.
                 * @member {evohime.desktop.v1.Handshake.$Properties|null|undefined} handshake
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.handshake = null;

                /**
                 * CommandEnvelope replayEvents.
                 * @member {evohime.desktop.v1.ReplayEvents.$Properties|null|undefined} replayEvents
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.replayEvents = null;

                /**
                 * CommandEnvelope startTask.
                 * @member {evohime.desktop.v1.StartTask.$Properties|null|undefined} startTask
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.startTask = null;

                /**
                 * CommandEnvelope stopTask.
                 * @member {evohime.desktop.v1.StopTask.$Properties|null|undefined} stopTask
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.stopTask = null;

                /**
                 * CommandEnvelope resolveApproval.
                 * @member {evohime.desktop.v1.ResolveApproval.$Properties|null|undefined} resolveApproval
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.resolveApproval = null;

                /**
                 * CommandEnvelope modelConfig.
                 * @member {evohime.desktop.v1.ModelConfigRequest.$Properties|null|undefined} modelConfig
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.modelConfig = null;

                /**
                 * CommandEnvelope modelCatalog.
                 * @member {evohime.desktop.v1.ModelCatalogRequest.$Properties|null|undefined} modelCatalog
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.modelCatalog = null;

                /**
                 * CommandEnvelope permissionMode.
                 * @member {evohime.desktop.v1.PermissionModeRequest.$Properties|null|undefined} permissionMode
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.permissionMode = null;

                /**
                 * CommandEnvelope createProject.
                 * @member {evohime.desktop.v1.CreateProject.$Properties|null|undefined} createProject
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.createProject = null;

                /**
                 * CommandEnvelope createTask.
                 * @member {evohime.desktop.v1.CreateTask.$Properties|null|undefined} createTask
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.createTask = null;

                /**
                 * CommandEnvelope updateTaskStatus.
                 * @member {evohime.desktop.v1.UpdateTaskStatus.$Properties|null|undefined} updateTaskStatus
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.updateTaskStatus = null;

                /**
                 * CommandEnvelope addTaskEdge.
                 * @member {evohime.desktop.v1.AddTaskEdge.$Properties|null|undefined} addTaskEdge
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.addTaskEdge = null;

                /**
                 * CommandEnvelope getTaskGraph.
                 * @member {evohime.desktop.v1.GetTaskGraph.$Properties|null|undefined} getTaskGraph
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.getTaskGraph = null;

                /**
                 * CommandEnvelope nextReadyTask.
                 * @member {evohime.desktop.v1.NextReadyTask.$Properties|null|undefined} nextReadyTask
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.nextReadyTask = null;

                /**
                 * CommandEnvelope importPrd.
                 * @member {evohime.desktop.v1.ImportPrd.$Properties|null|undefined} importPrd
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.importPrd = null;

                /**
                 * CommandEnvelope getTaskHistory.
                 * @member {evohime.desktop.v1.GetTaskHistory.$Properties|null|undefined} getTaskHistory
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.getTaskHistory = null;

                /**
                 * CommandEnvelope getTaskContext.
                 * @member {evohime.desktop.v1.GetTaskContext.$Properties|null|undefined} getTaskContext
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.getTaskContext = null;

                /**
                 * CommandEnvelope getTaskPlanSpec.
                 * @member {evohime.desktop.v1.GetTaskPlanSpec.$Properties|null|undefined} getTaskPlanSpec
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.getTaskPlanSpec = null;

                /**
                 * CommandEnvelope applyApprovedBuild.
                 * @member {evohime.desktop.v1.ApplyApprovedBuild.$Properties|null|undefined} applyApprovedBuild
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.applyApprovedBuild = null;

                /**
                 * CommandEnvelope prepareBuild.
                 * @member {evohime.desktop.v1.PrepareBuild.$Properties|null|undefined} prepareBuild
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.prepareBuild = null;

                /**
                 * CommandEnvelope getTaskSnapshot.
                 * @member {evohime.desktop.v1.GetTaskSnapshot.$Properties|null|undefined} getTaskSnapshot
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.getTaskSnapshot = null;

                /**
                 * CommandEnvelope restoreTaskSnapshot.
                 * @member {evohime.desktop.v1.RestoreTaskSnapshot.$Properties|null|undefined} restoreTaskSnapshot
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.restoreTaskSnapshot = null;

                /**
                 * CommandEnvelope getBuildPolicy.
                 * @member {evohime.desktop.v1.GetBuildPolicy.$Properties|null|undefined} getBuildPolicy
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.getBuildPolicy = null;

                /**
                 * CommandEnvelope saveBuildPolicy.
                 * @member {evohime.desktop.v1.SaveBuildPolicy.$Properties|null|undefined} saveBuildPolicy
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.saveBuildPolicy = null;

                /**
                 * CommandEnvelope resyncRequest.
                 * @member {evohime.desktop.v1.ResyncRequest.$Properties|null|undefined} resyncRequest
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.resyncRequest = null;

                /**
                 * CommandEnvelope runDoctor.
                 * @member {evohime.desktop.v1.RunDoctor.$Properties|null|undefined} runDoctor
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.runDoctor = null;

                /**
                 * CommandEnvelope saveResearchEvidence.
                 * @member {evohime.desktop.v1.SaveResearchEvidence.$Properties|null|undefined} saveResearchEvidence
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.saveResearchEvidence = null;

                /**
                 * CommandEnvelope listResearchEvidence.
                 * @member {evohime.desktop.v1.ListResearchEvidence.$Properties|null|undefined} listResearchEvidence
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.listResearchEvidence = null;

                /**
                 * CommandEnvelope createMemory.
                 * @member {evohime.desktop.v1.CreateMemory.$Properties|null|undefined} createMemory
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.createMemory = null;

                /**
                 * CommandEnvelope listMemory.
                 * @member {evohime.desktop.v1.ListMemory.$Properties|null|undefined} listMemory
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.listMemory = null;

                /**
                 * CommandEnvelope searchMemory.
                 * @member {evohime.desktop.v1.SearchMemory.$Properties|null|undefined} searchMemory
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.searchMemory = null;

                /**
                 * CommandEnvelope archiveMemory.
                 * @member {evohime.desktop.v1.ArchiveMemory.$Properties|null|undefined} archiveMemory
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.archiveMemory = null;

                /**
                 * CommandEnvelope forgetMemory.
                 * @member {evohime.desktop.v1.ForgetMemory.$Properties|null|undefined} forgetMemory
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.forgetMemory = null;

                /**
                 * CommandEnvelope installCapability.
                 * @member {evohime.desktop.v1.InstallCapability.$Properties|null|undefined} installCapability
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.installCapability = null;

                /**
                 * CommandEnvelope listCapabilities.
                 * @member {evohime.desktop.v1.ListCapabilities.$Properties|null|undefined} listCapabilities
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.listCapabilities = null;

                /**
                 * CommandEnvelope matchCapabilities.
                 * @member {evohime.desktop.v1.MatchCapabilities.$Properties|null|undefined} matchCapabilities
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.matchCapabilities = null;

                /**
                 * CommandEnvelope removeCapability.
                 * @member {evohime.desktop.v1.RemoveCapability.$Properties|null|undefined} removeCapability
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.removeCapability = null;

                /**
                 * CommandEnvelope requestChildHandoff.
                 * @member {evohime.desktop.v1.RequestChildHandoff.$Properties|null|undefined} requestChildHandoff
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.requestChildHandoff = null;

                /**
                 * CommandEnvelope listChildHandoffs.
                 * @member {evohime.desktop.v1.ListChildHandoffs.$Properties|null|undefined} listChildHandoffs
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.listChildHandoffs = null;

                /**
                 * CommandEnvelope submitChildRequest.
                 * @member {evohime.desktop.v1.SubmitChildRequest.$Properties|null|undefined} submitChildRequest
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.submitChildRequest = null;

                /**
                 * CommandEnvelope submitChildReport.
                 * @member {evohime.desktop.v1.SubmitChildReport.$Properties|null|undefined} submitChildReport
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.submitChildReport = null;

                /**
                 * CommandEnvelope runResearchFetch.
                 * @member {evohime.desktop.v1.RunResearchFetch.$Properties|null|undefined} runResearchFetch
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.runResearchFetch = null;

                /**
                 * CommandEnvelope listWorkspace.
                 * @member {evohime.desktop.v1.ListWorkspace.$Properties|null|undefined} listWorkspace
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.listWorkspace = null;

                /**
                 * CommandEnvelope readWorkspaceFile.
                 * @member {evohime.desktop.v1.ReadWorkspaceFile.$Properties|null|undefined} readWorkspaceFile
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.readWorkspaceFile = null;

                /**
                 * CommandEnvelope gitStatus.
                 * @member {evohime.desktop.v1.GitStatus.$Properties|null|undefined} gitStatus
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.gitStatus = null;

                /**
                 * CommandEnvelope gitDiff.
                 * @member {evohime.desktop.v1.GitDiff.$Properties|null|undefined} gitDiff
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.gitDiff = null;

                /**
                 * CommandEnvelope terminalExecute.
                 * @member {evohime.desktop.v1.TerminalExecute.$Properties|null|undefined} terminalExecute
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.terminalExecute = null;

                /**
                 * CommandEnvelope exportDoctorLogs.
                 * @member {evohime.desktop.v1.ExportDoctorLogs.$Properties|null|undefined} exportDoctorLogs
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.exportDoctorLogs = null;

                /**
                 * CommandEnvelope getCapabilitySelection.
                 * @member {evohime.desktop.v1.GetCapabilitySelection.$Properties|null|undefined} getCapabilitySelection
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.getCapabilitySelection = null;

                /**
                 * CommandEnvelope pinCapabilitySelection.
                 * @member {evohime.desktop.v1.PinCapabilitySelection.$Properties|null|undefined} pinCapabilitySelection
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.pinCapabilitySelection = null;

                /**
                 * CommandEnvelope replaceCapabilitySelection.
                 * @member {evohime.desktop.v1.ReplaceCapabilitySelection.$Properties|null|undefined} replaceCapabilitySelection
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.replaceCapabilitySelection = null;

                /**
                 * CommandEnvelope submitFeedback.
                 * @member {evohime.desktop.v1.SubmitFeedback.$Properties|null|undefined} submitFeedback
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.submitFeedback = null;

                /**
                 * CommandEnvelope listFeedback.
                 * @member {evohime.desktop.v1.ListFeedback.$Properties|null|undefined} listFeedback
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.listFeedback = null;

                /**
                 * CommandEnvelope createDatabaseBackup.
                 * @member {evohime.desktop.v1.CreateDatabaseBackup.$Properties|null|undefined} createDatabaseBackup
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.createDatabaseBackup = null;

                /**
                 * CommandEnvelope prepareDatabaseRestore.
                 * @member {evohime.desktop.v1.PrepareDatabaseRestore.$Properties|null|undefined} prepareDatabaseRestore
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.prepareDatabaseRestore = null;

                /**
                 * CommandEnvelope restoreDatabase.
                 * @member {evohime.desktop.v1.RestoreDatabase.$Properties|null|undefined} restoreDatabase
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.restoreDatabase = null;

                /**
                 * CommandEnvelope selectModel.
                 * @member {evohime.desktop.v1.SelectModelRequest.$Properties|null|undefined} selectModel
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.selectModel = null;

                /**
                 * CommandEnvelope cancelDatabaseOperation.
                 * @member {evohime.desktop.v1.CancelDatabaseOperation.$Properties|null|undefined} cancelDatabaseOperation
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                CommandEnvelope.prototype.cancelDatabaseOperation = null;

                // OneOf field names bound to virtual getters and setters
                let $oneOfFields;

                /**
                 * CommandEnvelope command.
                 * @member {"handshake"|"replayEvents"|"startTask"|"stopTask"|"resolveApproval"|"modelConfig"|"modelCatalog"|"permissionMode"|"createProject"|"createTask"|"updateTaskStatus"|"addTaskEdge"|"getTaskGraph"|"nextReadyTask"|"importPrd"|"getTaskHistory"|"getTaskContext"|"getTaskPlanSpec"|"applyApprovedBuild"|"prepareBuild"|"getTaskSnapshot"|"restoreTaskSnapshot"|"getBuildPolicy"|"saveBuildPolicy"|"resyncRequest"|"runDoctor"|"saveResearchEvidence"|"listResearchEvidence"|"createMemory"|"listMemory"|"searchMemory"|"archiveMemory"|"forgetMemory"|"installCapability"|"listCapabilities"|"matchCapabilities"|"removeCapability"|"requestChildHandoff"|"listChildHandoffs"|"submitChildRequest"|"submitChildReport"|"runResearchFetch"|"listWorkspace"|"readWorkspaceFile"|"gitStatus"|"gitDiff"|"terminalExecute"|"exportDoctorLogs"|"getCapabilitySelection"|"pinCapabilitySelection"|"replaceCapabilitySelection"|"submitFeedback"|"listFeedback"|"createDatabaseBackup"|"prepareDatabaseRestore"|"restoreDatabase"|"selectModel"|"cancelDatabaseOperation"|undefined} command
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @instance
                 */
                $Object.defineProperty(CommandEnvelope.prototype, "command", {
                    get: $util.oneOfGetter($oneOfFields = ["handshake", "replayEvents", "startTask", "stopTask", "resolveApproval", "modelConfig", "modelCatalog", "permissionMode", "createProject", "createTask", "updateTaskStatus", "addTaskEdge", "getTaskGraph", "nextReadyTask", "importPrd", "getTaskHistory", "getTaskContext", "getTaskPlanSpec", "applyApprovedBuild", "prepareBuild", "getTaskSnapshot", "restoreTaskSnapshot", "getBuildPolicy", "saveBuildPolicy", "resyncRequest", "runDoctor", "saveResearchEvidence", "listResearchEvidence", "createMemory", "listMemory", "searchMemory", "archiveMemory", "forgetMemory", "installCapability", "listCapabilities", "matchCapabilities", "removeCapability", "requestChildHandoff", "listChildHandoffs", "submitChildRequest", "submitChildReport", "runResearchFetch", "listWorkspace", "readWorkspaceFile", "gitStatus", "gitDiff", "terminalExecute", "exportDoctorLogs", "getCapabilitySelection", "pinCapabilitySelection", "replaceCapabilitySelection", "submitFeedback", "listFeedback", "createDatabaseBackup", "prepareDatabaseRestore", "restoreDatabase", "selectModel", "cancelDatabaseOperation"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Encodes the specified CommandEnvelope message. Does not implicitly {@link evohime.desktop.v1.CommandEnvelope.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @static
                 * @param {evohime.desktop.v1.CommandEnvelope.$Properties} message CommandEnvelope message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CommandEnvelope.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.protocol != null && $Object.hasOwnProperty.call(message, "protocol"))
                        $root.evohime.desktop.v1.ProtocolVersion.encode(message.protocol, writer.uint32(/* id 1, wireType 2 =*/10).fork(), _depth + 1).ldelim();
                    if (message.requestId != null && $Object.hasOwnProperty.call(message, "requestId") && message.requestId !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.requestId);
                    if (message.clientId != null && $Object.hasOwnProperty.call(message, "clientId") && message.clientId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.clientId);
                    if (message.coreInstanceId != null && $Object.hasOwnProperty.call(message, "coreInstanceId") && message.coreInstanceId !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.coreInstanceId);
                    if (message.sessionEpoch != null && $Object.hasOwnProperty.call(message, "sessionEpoch") && (typeof message.sessionEpoch === "object" ? message.sessionEpoch.low || message.sessionEpoch.high : message.sessionEpoch !== 0))
                        writer.uint32(/* id 5, wireType 0 =*/40).uint64(message.sessionEpoch);
                    if (message.handshake != null && $Object.hasOwnProperty.call(message, "handshake"))
                        $root.evohime.desktop.v1.Handshake.encode(message.handshake, writer.uint32(/* id 10, wireType 2 =*/82).fork(), _depth + 1).ldelim();
                    if (message.replayEvents != null && $Object.hasOwnProperty.call(message, "replayEvents"))
                        $root.evohime.desktop.v1.ReplayEvents.encode(message.replayEvents, writer.uint32(/* id 11, wireType 2 =*/90).fork(), _depth + 1).ldelim();
                    if (message.startTask != null && $Object.hasOwnProperty.call(message, "startTask"))
                        $root.evohime.desktop.v1.StartTask.encode(message.startTask, writer.uint32(/* id 12, wireType 2 =*/98).fork(), _depth + 1).ldelim();
                    if (message.stopTask != null && $Object.hasOwnProperty.call(message, "stopTask"))
                        $root.evohime.desktop.v1.StopTask.encode(message.stopTask, writer.uint32(/* id 13, wireType 2 =*/106).fork(), _depth + 1).ldelim();
                    if (message.resolveApproval != null && $Object.hasOwnProperty.call(message, "resolveApproval"))
                        $root.evohime.desktop.v1.ResolveApproval.encode(message.resolveApproval, writer.uint32(/* id 14, wireType 2 =*/114).fork(), _depth + 1).ldelim();
                    if (message.modelConfig != null && $Object.hasOwnProperty.call(message, "modelConfig"))
                        $root.evohime.desktop.v1.ModelConfigRequest.encode(message.modelConfig, writer.uint32(/* id 15, wireType 2 =*/122).fork(), _depth + 1).ldelim();
                    if (message.modelCatalog != null && $Object.hasOwnProperty.call(message, "modelCatalog"))
                        $root.evohime.desktop.v1.ModelCatalogRequest.encode(message.modelCatalog, writer.uint32(/* id 16, wireType 2 =*/130).fork(), _depth + 1).ldelim();
                    if (message.permissionMode != null && $Object.hasOwnProperty.call(message, "permissionMode"))
                        $root.evohime.desktop.v1.PermissionModeRequest.encode(message.permissionMode, writer.uint32(/* id 17, wireType 2 =*/138).fork(), _depth + 1).ldelim();
                    if (message.createProject != null && $Object.hasOwnProperty.call(message, "createProject"))
                        $root.evohime.desktop.v1.CreateProject.encode(message.createProject, writer.uint32(/* id 18, wireType 2 =*/146).fork(), _depth + 1).ldelim();
                    if (message.createTask != null && $Object.hasOwnProperty.call(message, "createTask"))
                        $root.evohime.desktop.v1.CreateTask.encode(message.createTask, writer.uint32(/* id 19, wireType 2 =*/154).fork(), _depth + 1).ldelim();
                    if (message.updateTaskStatus != null && $Object.hasOwnProperty.call(message, "updateTaskStatus"))
                        $root.evohime.desktop.v1.UpdateTaskStatus.encode(message.updateTaskStatus, writer.uint32(/* id 20, wireType 2 =*/162).fork(), _depth + 1).ldelim();
                    if (message.addTaskEdge != null && $Object.hasOwnProperty.call(message, "addTaskEdge"))
                        $root.evohime.desktop.v1.AddTaskEdge.encode(message.addTaskEdge, writer.uint32(/* id 21, wireType 2 =*/170).fork(), _depth + 1).ldelim();
                    if (message.getTaskGraph != null && $Object.hasOwnProperty.call(message, "getTaskGraph"))
                        $root.evohime.desktop.v1.GetTaskGraph.encode(message.getTaskGraph, writer.uint32(/* id 22, wireType 2 =*/178).fork(), _depth + 1).ldelim();
                    if (message.nextReadyTask != null && $Object.hasOwnProperty.call(message, "nextReadyTask"))
                        $root.evohime.desktop.v1.NextReadyTask.encode(message.nextReadyTask, writer.uint32(/* id 23, wireType 2 =*/186).fork(), _depth + 1).ldelim();
                    if (message.importPrd != null && $Object.hasOwnProperty.call(message, "importPrd"))
                        $root.evohime.desktop.v1.ImportPrd.encode(message.importPrd, writer.uint32(/* id 24, wireType 2 =*/194).fork(), _depth + 1).ldelim();
                    if (message.getTaskHistory != null && $Object.hasOwnProperty.call(message, "getTaskHistory"))
                        $root.evohime.desktop.v1.GetTaskHistory.encode(message.getTaskHistory, writer.uint32(/* id 25, wireType 2 =*/202).fork(), _depth + 1).ldelim();
                    if (message.getTaskContext != null && $Object.hasOwnProperty.call(message, "getTaskContext"))
                        $root.evohime.desktop.v1.GetTaskContext.encode(message.getTaskContext, writer.uint32(/* id 26, wireType 2 =*/210).fork(), _depth + 1).ldelim();
                    if (message.getTaskPlanSpec != null && $Object.hasOwnProperty.call(message, "getTaskPlanSpec"))
                        $root.evohime.desktop.v1.GetTaskPlanSpec.encode(message.getTaskPlanSpec, writer.uint32(/* id 27, wireType 2 =*/218).fork(), _depth + 1).ldelim();
                    if (message.applyApprovedBuild != null && $Object.hasOwnProperty.call(message, "applyApprovedBuild"))
                        $root.evohime.desktop.v1.ApplyApprovedBuild.encode(message.applyApprovedBuild, writer.uint32(/* id 28, wireType 2 =*/226).fork(), _depth + 1).ldelim();
                    if (message.prepareBuild != null && $Object.hasOwnProperty.call(message, "prepareBuild"))
                        $root.evohime.desktop.v1.PrepareBuild.encode(message.prepareBuild, writer.uint32(/* id 29, wireType 2 =*/234).fork(), _depth + 1).ldelim();
                    if (message.getTaskSnapshot != null && $Object.hasOwnProperty.call(message, "getTaskSnapshot"))
                        $root.evohime.desktop.v1.GetTaskSnapshot.encode(message.getTaskSnapshot, writer.uint32(/* id 30, wireType 2 =*/242).fork(), _depth + 1).ldelim();
                    if (message.restoreTaskSnapshot != null && $Object.hasOwnProperty.call(message, "restoreTaskSnapshot"))
                        $root.evohime.desktop.v1.RestoreTaskSnapshot.encode(message.restoreTaskSnapshot, writer.uint32(/* id 31, wireType 2 =*/250).fork(), _depth + 1).ldelim();
                    if (message.getBuildPolicy != null && $Object.hasOwnProperty.call(message, "getBuildPolicy"))
                        $root.evohime.desktop.v1.GetBuildPolicy.encode(message.getBuildPolicy, writer.uint32(/* id 32, wireType 2 =*/258).fork(), _depth + 1).ldelim();
                    if (message.saveBuildPolicy != null && $Object.hasOwnProperty.call(message, "saveBuildPolicy"))
                        $root.evohime.desktop.v1.SaveBuildPolicy.encode(message.saveBuildPolicy, writer.uint32(/* id 33, wireType 2 =*/266).fork(), _depth + 1).ldelim();
                    if (message.resyncRequest != null && $Object.hasOwnProperty.call(message, "resyncRequest"))
                        $root.evohime.desktop.v1.ResyncRequest.encode(message.resyncRequest, writer.uint32(/* id 34, wireType 2 =*/274).fork(), _depth + 1).ldelim();
                    if (message.runDoctor != null && $Object.hasOwnProperty.call(message, "runDoctor"))
                        $root.evohime.desktop.v1.RunDoctor.encode(message.runDoctor, writer.uint32(/* id 35, wireType 2 =*/282).fork(), _depth + 1).ldelim();
                    if (message.saveResearchEvidence != null && $Object.hasOwnProperty.call(message, "saveResearchEvidence"))
                        $root.evohime.desktop.v1.SaveResearchEvidence.encode(message.saveResearchEvidence, writer.uint32(/* id 36, wireType 2 =*/290).fork(), _depth + 1).ldelim();
                    if (message.listResearchEvidence != null && $Object.hasOwnProperty.call(message, "listResearchEvidence"))
                        $root.evohime.desktop.v1.ListResearchEvidence.encode(message.listResearchEvidence, writer.uint32(/* id 37, wireType 2 =*/298).fork(), _depth + 1).ldelim();
                    if (message.createMemory != null && $Object.hasOwnProperty.call(message, "createMemory"))
                        $root.evohime.desktop.v1.CreateMemory.encode(message.createMemory, writer.uint32(/* id 38, wireType 2 =*/306).fork(), _depth + 1).ldelim();
                    if (message.listMemory != null && $Object.hasOwnProperty.call(message, "listMemory"))
                        $root.evohime.desktop.v1.ListMemory.encode(message.listMemory, writer.uint32(/* id 39, wireType 2 =*/314).fork(), _depth + 1).ldelim();
                    if (message.searchMemory != null && $Object.hasOwnProperty.call(message, "searchMemory"))
                        $root.evohime.desktop.v1.SearchMemory.encode(message.searchMemory, writer.uint32(/* id 40, wireType 2 =*/322).fork(), _depth + 1).ldelim();
                    if (message.archiveMemory != null && $Object.hasOwnProperty.call(message, "archiveMemory"))
                        $root.evohime.desktop.v1.ArchiveMemory.encode(message.archiveMemory, writer.uint32(/* id 41, wireType 2 =*/330).fork(), _depth + 1).ldelim();
                    if (message.forgetMemory != null && $Object.hasOwnProperty.call(message, "forgetMemory"))
                        $root.evohime.desktop.v1.ForgetMemory.encode(message.forgetMemory, writer.uint32(/* id 42, wireType 2 =*/338).fork(), _depth + 1).ldelim();
                    if (message.installCapability != null && $Object.hasOwnProperty.call(message, "installCapability"))
                        $root.evohime.desktop.v1.InstallCapability.encode(message.installCapability, writer.uint32(/* id 43, wireType 2 =*/346).fork(), _depth + 1).ldelim();
                    if (message.listCapabilities != null && $Object.hasOwnProperty.call(message, "listCapabilities"))
                        $root.evohime.desktop.v1.ListCapabilities.encode(message.listCapabilities, writer.uint32(/* id 44, wireType 2 =*/354).fork(), _depth + 1).ldelim();
                    if (message.matchCapabilities != null && $Object.hasOwnProperty.call(message, "matchCapabilities"))
                        $root.evohime.desktop.v1.MatchCapabilities.encode(message.matchCapabilities, writer.uint32(/* id 45, wireType 2 =*/362).fork(), _depth + 1).ldelim();
                    if (message.removeCapability != null && $Object.hasOwnProperty.call(message, "removeCapability"))
                        $root.evohime.desktop.v1.RemoveCapability.encode(message.removeCapability, writer.uint32(/* id 46, wireType 2 =*/370).fork(), _depth + 1).ldelim();
                    if (message.requestChildHandoff != null && $Object.hasOwnProperty.call(message, "requestChildHandoff"))
                        $root.evohime.desktop.v1.RequestChildHandoff.encode(message.requestChildHandoff, writer.uint32(/* id 47, wireType 2 =*/378).fork(), _depth + 1).ldelim();
                    if (message.listChildHandoffs != null && $Object.hasOwnProperty.call(message, "listChildHandoffs"))
                        $root.evohime.desktop.v1.ListChildHandoffs.encode(message.listChildHandoffs, writer.uint32(/* id 48, wireType 2 =*/386).fork(), _depth + 1).ldelim();
                    if (message.submitChildRequest != null && $Object.hasOwnProperty.call(message, "submitChildRequest"))
                        $root.evohime.desktop.v1.SubmitChildRequest.encode(message.submitChildRequest, writer.uint32(/* id 49, wireType 2 =*/394).fork(), _depth + 1).ldelim();
                    if (message.submitChildReport != null && $Object.hasOwnProperty.call(message, "submitChildReport"))
                        $root.evohime.desktop.v1.SubmitChildReport.encode(message.submitChildReport, writer.uint32(/* id 50, wireType 2 =*/402).fork(), _depth + 1).ldelim();
                    if (message.runResearchFetch != null && $Object.hasOwnProperty.call(message, "runResearchFetch"))
                        $root.evohime.desktop.v1.RunResearchFetch.encode(message.runResearchFetch, writer.uint32(/* id 51, wireType 2 =*/410).fork(), _depth + 1).ldelim();
                    if (message.listWorkspace != null && $Object.hasOwnProperty.call(message, "listWorkspace"))
                        $root.evohime.desktop.v1.ListWorkspace.encode(message.listWorkspace, writer.uint32(/* id 52, wireType 2 =*/418).fork(), _depth + 1).ldelim();
                    if (message.readWorkspaceFile != null && $Object.hasOwnProperty.call(message, "readWorkspaceFile"))
                        $root.evohime.desktop.v1.ReadWorkspaceFile.encode(message.readWorkspaceFile, writer.uint32(/* id 53, wireType 2 =*/426).fork(), _depth + 1).ldelim();
                    if (message.gitStatus != null && $Object.hasOwnProperty.call(message, "gitStatus"))
                        $root.evohime.desktop.v1.GitStatus.encode(message.gitStatus, writer.uint32(/* id 54, wireType 2 =*/434).fork(), _depth + 1).ldelim();
                    if (message.gitDiff != null && $Object.hasOwnProperty.call(message, "gitDiff"))
                        $root.evohime.desktop.v1.GitDiff.encode(message.gitDiff, writer.uint32(/* id 55, wireType 2 =*/442).fork(), _depth + 1).ldelim();
                    if (message.terminalExecute != null && $Object.hasOwnProperty.call(message, "terminalExecute"))
                        $root.evohime.desktop.v1.TerminalExecute.encode(message.terminalExecute, writer.uint32(/* id 56, wireType 2 =*/450).fork(), _depth + 1).ldelim();
                    if (message.exportDoctorLogs != null && $Object.hasOwnProperty.call(message, "exportDoctorLogs"))
                        $root.evohime.desktop.v1.ExportDoctorLogs.encode(message.exportDoctorLogs, writer.uint32(/* id 57, wireType 2 =*/458).fork(), _depth + 1).ldelim();
                    if (message.getCapabilitySelection != null && $Object.hasOwnProperty.call(message, "getCapabilitySelection"))
                        $root.evohime.desktop.v1.GetCapabilitySelection.encode(message.getCapabilitySelection, writer.uint32(/* id 58, wireType 2 =*/466).fork(), _depth + 1).ldelim();
                    if (message.pinCapabilitySelection != null && $Object.hasOwnProperty.call(message, "pinCapabilitySelection"))
                        $root.evohime.desktop.v1.PinCapabilitySelection.encode(message.pinCapabilitySelection, writer.uint32(/* id 59, wireType 2 =*/474).fork(), _depth + 1).ldelim();
                    if (message.replaceCapabilitySelection != null && $Object.hasOwnProperty.call(message, "replaceCapabilitySelection"))
                        $root.evohime.desktop.v1.ReplaceCapabilitySelection.encode(message.replaceCapabilitySelection, writer.uint32(/* id 60, wireType 2 =*/482).fork(), _depth + 1).ldelim();
                    if (message.submitFeedback != null && $Object.hasOwnProperty.call(message, "submitFeedback"))
                        $root.evohime.desktop.v1.SubmitFeedback.encode(message.submitFeedback, writer.uint32(/* id 61, wireType 2 =*/490).fork(), _depth + 1).ldelim();
                    if (message.listFeedback != null && $Object.hasOwnProperty.call(message, "listFeedback"))
                        $root.evohime.desktop.v1.ListFeedback.encode(message.listFeedback, writer.uint32(/* id 62, wireType 2 =*/498).fork(), _depth + 1).ldelim();
                    if (message.createDatabaseBackup != null && $Object.hasOwnProperty.call(message, "createDatabaseBackup"))
                        $root.evohime.desktop.v1.CreateDatabaseBackup.encode(message.createDatabaseBackup, writer.uint32(/* id 63, wireType 2 =*/506).fork(), _depth + 1).ldelim();
                    if (message.prepareDatabaseRestore != null && $Object.hasOwnProperty.call(message, "prepareDatabaseRestore"))
                        $root.evohime.desktop.v1.PrepareDatabaseRestore.encode(message.prepareDatabaseRestore, writer.uint32(/* id 64, wireType 2 =*/514).fork(), _depth + 1).ldelim();
                    if (message.restoreDatabase != null && $Object.hasOwnProperty.call(message, "restoreDatabase"))
                        $root.evohime.desktop.v1.RestoreDatabase.encode(message.restoreDatabase, writer.uint32(/* id 65, wireType 2 =*/522).fork(), _depth + 1).ldelim();
                    if (message.selectModel != null && $Object.hasOwnProperty.call(message, "selectModel"))
                        $root.evohime.desktop.v1.SelectModelRequest.encode(message.selectModel, writer.uint32(/* id 66, wireType 2 =*/530).fork(), _depth + 1).ldelim();
                    if (message.cancelDatabaseOperation != null && $Object.hasOwnProperty.call(message, "cancelDatabaseOperation"))
                        $root.evohime.desktop.v1.CancelDatabaseOperation.encode(message.cancelDatabaseOperation, writer.uint32(/* id 67, wireType 2 =*/538).fork(), _depth + 1).ldelim();
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a CommandEnvelope message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CommandEnvelope & evohime.desktop.v1.CommandEnvelope.$Shape} CommandEnvelope
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CommandEnvelope.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.CommandEnvelope(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                message.protocol = $root.evohime.desktop.v1.ProtocolVersion.decode(reader, reader.uint32(), $undefined, _depth + 1, message.protocol);
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.requestId = value;
                                else
                                    delete message.requestId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.clientId = value;
                                else
                                    delete message.clientId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.coreInstanceId = value;
                                else
                                    delete message.coreInstanceId;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.sessionEpoch = value;
                                else
                                    delete message.sessionEpoch;
                                continue;
                            }
                        case 10: {
                                if (wireType !== 2)
                                    break;
                                message.handshake = $root.evohime.desktop.v1.Handshake.decode(reader, reader.uint32(), $undefined, _depth + 1, message.handshake);
                                message.command = "handshake";
                                continue;
                            }
                        case 11: {
                                if (wireType !== 2)
                                    break;
                                message.replayEvents = $root.evohime.desktop.v1.ReplayEvents.decode(reader, reader.uint32(), $undefined, _depth + 1, message.replayEvents);
                                message.command = "replayEvents";
                                continue;
                            }
                        case 12: {
                                if (wireType !== 2)
                                    break;
                                message.startTask = $root.evohime.desktop.v1.StartTask.decode(reader, reader.uint32(), $undefined, _depth + 1, message.startTask);
                                message.command = "startTask";
                                continue;
                            }
                        case 13: {
                                if (wireType !== 2)
                                    break;
                                message.stopTask = $root.evohime.desktop.v1.StopTask.decode(reader, reader.uint32(), $undefined, _depth + 1, message.stopTask);
                                message.command = "stopTask";
                                continue;
                            }
                        case 14: {
                                if (wireType !== 2)
                                    break;
                                message.resolveApproval = $root.evohime.desktop.v1.ResolveApproval.decode(reader, reader.uint32(), $undefined, _depth + 1, message.resolveApproval);
                                message.command = "resolveApproval";
                                continue;
                            }
                        case 15: {
                                if (wireType !== 2)
                                    break;
                                message.modelConfig = $root.evohime.desktop.v1.ModelConfigRequest.decode(reader, reader.uint32(), $undefined, _depth + 1, message.modelConfig);
                                message.command = "modelConfig";
                                continue;
                            }
                        case 16: {
                                if (wireType !== 2)
                                    break;
                                message.modelCatalog = $root.evohime.desktop.v1.ModelCatalogRequest.decode(reader, reader.uint32(), $undefined, _depth + 1, message.modelCatalog);
                                message.command = "modelCatalog";
                                continue;
                            }
                        case 17: {
                                if (wireType !== 2)
                                    break;
                                message.permissionMode = $root.evohime.desktop.v1.PermissionModeRequest.decode(reader, reader.uint32(), $undefined, _depth + 1, message.permissionMode);
                                message.command = "permissionMode";
                                continue;
                            }
                        case 18: {
                                if (wireType !== 2)
                                    break;
                                message.createProject = $root.evohime.desktop.v1.CreateProject.decode(reader, reader.uint32(), $undefined, _depth + 1, message.createProject);
                                message.command = "createProject";
                                continue;
                            }
                        case 19: {
                                if (wireType !== 2)
                                    break;
                                message.createTask = $root.evohime.desktop.v1.CreateTask.decode(reader, reader.uint32(), $undefined, _depth + 1, message.createTask);
                                message.command = "createTask";
                                continue;
                            }
                        case 20: {
                                if (wireType !== 2)
                                    break;
                                message.updateTaskStatus = $root.evohime.desktop.v1.UpdateTaskStatus.decode(reader, reader.uint32(), $undefined, _depth + 1, message.updateTaskStatus);
                                message.command = "updateTaskStatus";
                                continue;
                            }
                        case 21: {
                                if (wireType !== 2)
                                    break;
                                message.addTaskEdge = $root.evohime.desktop.v1.AddTaskEdge.decode(reader, reader.uint32(), $undefined, _depth + 1, message.addTaskEdge);
                                message.command = "addTaskEdge";
                                continue;
                            }
                        case 22: {
                                if (wireType !== 2)
                                    break;
                                message.getTaskGraph = $root.evohime.desktop.v1.GetTaskGraph.decode(reader, reader.uint32(), $undefined, _depth + 1, message.getTaskGraph);
                                message.command = "getTaskGraph";
                                continue;
                            }
                        case 23: {
                                if (wireType !== 2)
                                    break;
                                message.nextReadyTask = $root.evohime.desktop.v1.NextReadyTask.decode(reader, reader.uint32(), $undefined, _depth + 1, message.nextReadyTask);
                                message.command = "nextReadyTask";
                                continue;
                            }
                        case 24: {
                                if (wireType !== 2)
                                    break;
                                message.importPrd = $root.evohime.desktop.v1.ImportPrd.decode(reader, reader.uint32(), $undefined, _depth + 1, message.importPrd);
                                message.command = "importPrd";
                                continue;
                            }
                        case 25: {
                                if (wireType !== 2)
                                    break;
                                message.getTaskHistory = $root.evohime.desktop.v1.GetTaskHistory.decode(reader, reader.uint32(), $undefined, _depth + 1, message.getTaskHistory);
                                message.command = "getTaskHistory";
                                continue;
                            }
                        case 26: {
                                if (wireType !== 2)
                                    break;
                                message.getTaskContext = $root.evohime.desktop.v1.GetTaskContext.decode(reader, reader.uint32(), $undefined, _depth + 1, message.getTaskContext);
                                message.command = "getTaskContext";
                                continue;
                            }
                        case 27: {
                                if (wireType !== 2)
                                    break;
                                message.getTaskPlanSpec = $root.evohime.desktop.v1.GetTaskPlanSpec.decode(reader, reader.uint32(), $undefined, _depth + 1, message.getTaskPlanSpec);
                                message.command = "getTaskPlanSpec";
                                continue;
                            }
                        case 28: {
                                if (wireType !== 2)
                                    break;
                                message.applyApprovedBuild = $root.evohime.desktop.v1.ApplyApprovedBuild.decode(reader, reader.uint32(), $undefined, _depth + 1, message.applyApprovedBuild);
                                message.command = "applyApprovedBuild";
                                continue;
                            }
                        case 29: {
                                if (wireType !== 2)
                                    break;
                                message.prepareBuild = $root.evohime.desktop.v1.PrepareBuild.decode(reader, reader.uint32(), $undefined, _depth + 1, message.prepareBuild);
                                message.command = "prepareBuild";
                                continue;
                            }
                        case 30: {
                                if (wireType !== 2)
                                    break;
                                message.getTaskSnapshot = $root.evohime.desktop.v1.GetTaskSnapshot.decode(reader, reader.uint32(), $undefined, _depth + 1, message.getTaskSnapshot);
                                message.command = "getTaskSnapshot";
                                continue;
                            }
                        case 31: {
                                if (wireType !== 2)
                                    break;
                                message.restoreTaskSnapshot = $root.evohime.desktop.v1.RestoreTaskSnapshot.decode(reader, reader.uint32(), $undefined, _depth + 1, message.restoreTaskSnapshot);
                                message.command = "restoreTaskSnapshot";
                                continue;
                            }
                        case 32: {
                                if (wireType !== 2)
                                    break;
                                message.getBuildPolicy = $root.evohime.desktop.v1.GetBuildPolicy.decode(reader, reader.uint32(), $undefined, _depth + 1, message.getBuildPolicy);
                                message.command = "getBuildPolicy";
                                continue;
                            }
                        case 33: {
                                if (wireType !== 2)
                                    break;
                                message.saveBuildPolicy = $root.evohime.desktop.v1.SaveBuildPolicy.decode(reader, reader.uint32(), $undefined, _depth + 1, message.saveBuildPolicy);
                                message.command = "saveBuildPolicy";
                                continue;
                            }
                        case 34: {
                                if (wireType !== 2)
                                    break;
                                message.resyncRequest = $root.evohime.desktop.v1.ResyncRequest.decode(reader, reader.uint32(), $undefined, _depth + 1, message.resyncRequest);
                                message.command = "resyncRequest";
                                continue;
                            }
                        case 35: {
                                if (wireType !== 2)
                                    break;
                                message.runDoctor = $root.evohime.desktop.v1.RunDoctor.decode(reader, reader.uint32(), $undefined, _depth + 1, message.runDoctor);
                                message.command = "runDoctor";
                                continue;
                            }
                        case 36: {
                                if (wireType !== 2)
                                    break;
                                message.saveResearchEvidence = $root.evohime.desktop.v1.SaveResearchEvidence.decode(reader, reader.uint32(), $undefined, _depth + 1, message.saveResearchEvidence);
                                message.command = "saveResearchEvidence";
                                continue;
                            }
                        case 37: {
                                if (wireType !== 2)
                                    break;
                                message.listResearchEvidence = $root.evohime.desktop.v1.ListResearchEvidence.decode(reader, reader.uint32(), $undefined, _depth + 1, message.listResearchEvidence);
                                message.command = "listResearchEvidence";
                                continue;
                            }
                        case 38: {
                                if (wireType !== 2)
                                    break;
                                message.createMemory = $root.evohime.desktop.v1.CreateMemory.decode(reader, reader.uint32(), $undefined, _depth + 1, message.createMemory);
                                message.command = "createMemory";
                                continue;
                            }
                        case 39: {
                                if (wireType !== 2)
                                    break;
                                message.listMemory = $root.evohime.desktop.v1.ListMemory.decode(reader, reader.uint32(), $undefined, _depth + 1, message.listMemory);
                                message.command = "listMemory";
                                continue;
                            }
                        case 40: {
                                if (wireType !== 2)
                                    break;
                                message.searchMemory = $root.evohime.desktop.v1.SearchMemory.decode(reader, reader.uint32(), $undefined, _depth + 1, message.searchMemory);
                                message.command = "searchMemory";
                                continue;
                            }
                        case 41: {
                                if (wireType !== 2)
                                    break;
                                message.archiveMemory = $root.evohime.desktop.v1.ArchiveMemory.decode(reader, reader.uint32(), $undefined, _depth + 1, message.archiveMemory);
                                message.command = "archiveMemory";
                                continue;
                            }
                        case 42: {
                                if (wireType !== 2)
                                    break;
                                message.forgetMemory = $root.evohime.desktop.v1.ForgetMemory.decode(reader, reader.uint32(), $undefined, _depth + 1, message.forgetMemory);
                                message.command = "forgetMemory";
                                continue;
                            }
                        case 43: {
                                if (wireType !== 2)
                                    break;
                                message.installCapability = $root.evohime.desktop.v1.InstallCapability.decode(reader, reader.uint32(), $undefined, _depth + 1, message.installCapability);
                                message.command = "installCapability";
                                continue;
                            }
                        case 44: {
                                if (wireType !== 2)
                                    break;
                                message.listCapabilities = $root.evohime.desktop.v1.ListCapabilities.decode(reader, reader.uint32(), $undefined, _depth + 1, message.listCapabilities);
                                message.command = "listCapabilities";
                                continue;
                            }
                        case 45: {
                                if (wireType !== 2)
                                    break;
                                message.matchCapabilities = $root.evohime.desktop.v1.MatchCapabilities.decode(reader, reader.uint32(), $undefined, _depth + 1, message.matchCapabilities);
                                message.command = "matchCapabilities";
                                continue;
                            }
                        case 46: {
                                if (wireType !== 2)
                                    break;
                                message.removeCapability = $root.evohime.desktop.v1.RemoveCapability.decode(reader, reader.uint32(), $undefined, _depth + 1, message.removeCapability);
                                message.command = "removeCapability";
                                continue;
                            }
                        case 47: {
                                if (wireType !== 2)
                                    break;
                                message.requestChildHandoff = $root.evohime.desktop.v1.RequestChildHandoff.decode(reader, reader.uint32(), $undefined, _depth + 1, message.requestChildHandoff);
                                message.command = "requestChildHandoff";
                                continue;
                            }
                        case 48: {
                                if (wireType !== 2)
                                    break;
                                message.listChildHandoffs = $root.evohime.desktop.v1.ListChildHandoffs.decode(reader, reader.uint32(), $undefined, _depth + 1, message.listChildHandoffs);
                                message.command = "listChildHandoffs";
                                continue;
                            }
                        case 49: {
                                if (wireType !== 2)
                                    break;
                                message.submitChildRequest = $root.evohime.desktop.v1.SubmitChildRequest.decode(reader, reader.uint32(), $undefined, _depth + 1, message.submitChildRequest);
                                message.command = "submitChildRequest";
                                continue;
                            }
                        case 50: {
                                if (wireType !== 2)
                                    break;
                                message.submitChildReport = $root.evohime.desktop.v1.SubmitChildReport.decode(reader, reader.uint32(), $undefined, _depth + 1, message.submitChildReport);
                                message.command = "submitChildReport";
                                continue;
                            }
                        case 51: {
                                if (wireType !== 2)
                                    break;
                                message.runResearchFetch = $root.evohime.desktop.v1.RunResearchFetch.decode(reader, reader.uint32(), $undefined, _depth + 1, message.runResearchFetch);
                                message.command = "runResearchFetch";
                                continue;
                            }
                        case 52: {
                                if (wireType !== 2)
                                    break;
                                message.listWorkspace = $root.evohime.desktop.v1.ListWorkspace.decode(reader, reader.uint32(), $undefined, _depth + 1, message.listWorkspace);
                                message.command = "listWorkspace";
                                continue;
                            }
                        case 53: {
                                if (wireType !== 2)
                                    break;
                                message.readWorkspaceFile = $root.evohime.desktop.v1.ReadWorkspaceFile.decode(reader, reader.uint32(), $undefined, _depth + 1, message.readWorkspaceFile);
                                message.command = "readWorkspaceFile";
                                continue;
                            }
                        case 54: {
                                if (wireType !== 2)
                                    break;
                                message.gitStatus = $root.evohime.desktop.v1.GitStatus.decode(reader, reader.uint32(), $undefined, _depth + 1, message.gitStatus);
                                message.command = "gitStatus";
                                continue;
                            }
                        case 55: {
                                if (wireType !== 2)
                                    break;
                                message.gitDiff = $root.evohime.desktop.v1.GitDiff.decode(reader, reader.uint32(), $undefined, _depth + 1, message.gitDiff);
                                message.command = "gitDiff";
                                continue;
                            }
                        case 56: {
                                if (wireType !== 2)
                                    break;
                                message.terminalExecute = $root.evohime.desktop.v1.TerminalExecute.decode(reader, reader.uint32(), $undefined, _depth + 1, message.terminalExecute);
                                message.command = "terminalExecute";
                                continue;
                            }
                        case 57: {
                                if (wireType !== 2)
                                    break;
                                message.exportDoctorLogs = $root.evohime.desktop.v1.ExportDoctorLogs.decode(reader, reader.uint32(), $undefined, _depth + 1, message.exportDoctorLogs);
                                message.command = "exportDoctorLogs";
                                continue;
                            }
                        case 58: {
                                if (wireType !== 2)
                                    break;
                                message.getCapabilitySelection = $root.evohime.desktop.v1.GetCapabilitySelection.decode(reader, reader.uint32(), $undefined, _depth + 1, message.getCapabilitySelection);
                                message.command = "getCapabilitySelection";
                                continue;
                            }
                        case 59: {
                                if (wireType !== 2)
                                    break;
                                message.pinCapabilitySelection = $root.evohime.desktop.v1.PinCapabilitySelection.decode(reader, reader.uint32(), $undefined, _depth + 1, message.pinCapabilitySelection);
                                message.command = "pinCapabilitySelection";
                                continue;
                            }
                        case 60: {
                                if (wireType !== 2)
                                    break;
                                message.replaceCapabilitySelection = $root.evohime.desktop.v1.ReplaceCapabilitySelection.decode(reader, reader.uint32(), $undefined, _depth + 1, message.replaceCapabilitySelection);
                                message.command = "replaceCapabilitySelection";
                                continue;
                            }
                        case 61: {
                                if (wireType !== 2)
                                    break;
                                message.submitFeedback = $root.evohime.desktop.v1.SubmitFeedback.decode(reader, reader.uint32(), $undefined, _depth + 1, message.submitFeedback);
                                message.command = "submitFeedback";
                                continue;
                            }
                        case 62: {
                                if (wireType !== 2)
                                    break;
                                message.listFeedback = $root.evohime.desktop.v1.ListFeedback.decode(reader, reader.uint32(), $undefined, _depth + 1, message.listFeedback);
                                message.command = "listFeedback";
                                continue;
                            }
                        case 63: {
                                if (wireType !== 2)
                                    break;
                                message.createDatabaseBackup = $root.evohime.desktop.v1.CreateDatabaseBackup.decode(reader, reader.uint32(), $undefined, _depth + 1, message.createDatabaseBackup);
                                message.command = "createDatabaseBackup";
                                continue;
                            }
                        case 64: {
                                if (wireType !== 2)
                                    break;
                                message.prepareDatabaseRestore = $root.evohime.desktop.v1.PrepareDatabaseRestore.decode(reader, reader.uint32(), $undefined, _depth + 1, message.prepareDatabaseRestore);
                                message.command = "prepareDatabaseRestore";
                                continue;
                            }
                        case 65: {
                                if (wireType !== 2)
                                    break;
                                message.restoreDatabase = $root.evohime.desktop.v1.RestoreDatabase.decode(reader, reader.uint32(), $undefined, _depth + 1, message.restoreDatabase);
                                message.command = "restoreDatabase";
                                continue;
                            }
                        case 66: {
                                if (wireType !== 2)
                                    break;
                                message.selectModel = $root.evohime.desktop.v1.SelectModelRequest.decode(reader, reader.uint32(), $undefined, _depth + 1, message.selectModel);
                                message.command = "selectModel";
                                continue;
                            }
                        case 67: {
                                if (wireType !== 2)
                                    break;
                                message.cancelDatabaseOperation = $root.evohime.desktop.v1.CancelDatabaseOperation.decode(reader, reader.uint32(), $undefined, _depth + 1, message.cancelDatabaseOperation);
                                message.command = "cancelDatabaseOperation";
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for CommandEnvelope
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.CommandEnvelope
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                CommandEnvelope.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.CommandEnvelope";
                };

                return CommandEnvelope;
            })();

            v1.Ready = (function() {

                /**
                 * Properties of a Ready.
                 * @typedef {Object} evohime.desktop.v1.Ready.$Properties
                 * @property {evohime.desktop.v1.ProtocolVersion.$Properties|null} [protocol] Ready protocol
                 * @property {string|null} [coreVersion] Ready coreVersion
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a Ready.
                 * @memberof evohime.desktop.v1
                 * @interface IReady
                 * @augments evohime.desktop.v1.Ready.$Properties
                 * @deprecated Use evohime.desktop.v1.Ready.$Properties instead.
                 */

                /**
                 * Shape of a Ready.
                 * @typedef {evohime.desktop.v1.Ready.$Properties} evohime.desktop.v1.Ready.$Shape
                 */

                /**
                 * Constructs a new Ready.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a Ready.
                 * @constructor
                 * @param {evohime.desktop.v1.Ready.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const Ready = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * Ready protocol.
                 * @member {evohime.desktop.v1.ProtocolVersion.$Properties|null|undefined} protocol
                 * @memberof evohime.desktop.v1.Ready
                 * @instance
                 */
                Ready.prototype.protocol = null;

                /**
                 * Ready coreVersion.
                 * @member {string} coreVersion
                 * @memberof evohime.desktop.v1.Ready
                 * @instance
                 */
                Ready.prototype.coreVersion = "";

                /**
                 * Encodes the specified Ready message. Does not implicitly {@link evohime.desktop.v1.Ready.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.Ready
                 * @static
                 * @param {evohime.desktop.v1.Ready.$Properties} message Ready message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                Ready.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.protocol != null && $Object.hasOwnProperty.call(message, "protocol"))
                        $root.evohime.desktop.v1.ProtocolVersion.encode(message.protocol, writer.uint32(/* id 1, wireType 2 =*/10).fork(), _depth + 1).ldelim();
                    if (message.coreVersion != null && $Object.hasOwnProperty.call(message, "coreVersion") && message.coreVersion !== "")
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.coreVersion);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a Ready message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.Ready
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.Ready & evohime.desktop.v1.Ready.$Shape} Ready
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                Ready.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.Ready(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                message.protocol = $root.evohime.desktop.v1.ProtocolVersion.decode(reader, reader.uint32(), $undefined, _depth + 1, message.protocol);
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.coreVersion = value;
                                else
                                    delete message.coreVersion;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for Ready
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.Ready
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                Ready.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.Ready";
                };

                return Ready;
            })();

            v1.EventEnvelope = (function() {

                /**
                 * Properties of an EventEnvelope.
                 * @typedef {Object} evohime.desktop.v1.EventEnvelope.$Properties
                 * @property {evohime.desktop.v1.ProtocolVersion.$Properties|null} [protocol] EventEnvelope protocol
                 * @property {number|null} [sequenceId] EventEnvelope sequenceId
                 * @property {string|null} [taskId] EventEnvelope taskId
                 * @property {string|null} [eventType] EventEnvelope eventType
                 * @property {Uint8Array|null} [payload] EventEnvelope payload
                 * @property {string|null} [coreInstanceId] EventEnvelope coreInstanceId
                 * @property {number|null} [sessionEpoch] EventEnvelope sessionEpoch
                 * @property {evohime.desktop.v1.Ready.$Properties|null} [ready] EventEnvelope ready
                 * @property {evohime.desktop.v1.ReplayGap.$Properties|null} [replayGap] EventEnvelope replayGap
                 * @property {evohime.desktop.v1.FullSnapshot.$Properties|null} [fullSnapshot] EventEnvelope fullSnapshot
                 * @property {evohime.desktop.v1.AuthChallenge.$Properties|null} [authChallenge] EventEnvelope authChallenge
                 * @property {"ready"|"replayGap"|"fullSnapshot"|"authChallenge"} [event] EventEnvelope event
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of an EventEnvelope.
                 * @memberof evohime.desktop.v1
                 * @interface IEventEnvelope
                 * @augments evohime.desktop.v1.EventEnvelope.$Properties
                 * @deprecated Use evohime.desktop.v1.EventEnvelope.$Properties instead.
                 */

                /**
                 * Narrowed shape of an EventEnvelope.
                 * @typedef {{
                 *   protocol?: evohime.desktop.v1.ProtocolVersion.$Shape|null;
                 *   sequenceId?: number|null;
                 *   taskId?: string|null;
                 *   eventType?: string|null;
                 *   payload?: Uint8Array|null;
                 *   coreInstanceId?: string|null;
                 *   sessionEpoch?: number|null;
                 *   ready?: evohime.desktop.v1.Ready.$Shape|null;
                 *   replayGap?: evohime.desktop.v1.ReplayGap.$Shape|null;
                 *   fullSnapshot?: evohime.desktop.v1.FullSnapshot.$Shape|null;
                 *   authChallenge?: evohime.desktop.v1.AuthChallenge.$Shape|null;
                 *   $unknowns?: Array.<Uint8Array>;
                 * } & (
                 *   ({ event?: undefined; ready?: null; replayGap?: null; fullSnapshot?: null; authChallenge?: null }|{ event?: "ready"; ready: evohime.desktop.v1.Ready.$Shape; replayGap?: null; fullSnapshot?: null; authChallenge?: null }|{ event?: "replayGap"; ready?: null; replayGap: evohime.desktop.v1.ReplayGap.$Shape; fullSnapshot?: null; authChallenge?: null }|{ event?: "fullSnapshot"; ready?: null; replayGap?: null; fullSnapshot: evohime.desktop.v1.FullSnapshot.$Shape; authChallenge?: null }|{ event?: "authChallenge"; ready?: null; replayGap?: null; fullSnapshot?: null; authChallenge: evohime.desktop.v1.AuthChallenge.$Shape })
                 * )} evohime.desktop.v1.EventEnvelope.$Shape
                 */

                /**
                 * Constructs a new EventEnvelope.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents an EventEnvelope.
                 * @constructor
                 * @param {evohime.desktop.v1.EventEnvelope.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const EventEnvelope = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * EventEnvelope protocol.
                 * @member {evohime.desktop.v1.ProtocolVersion.$Properties|null|undefined} protocol
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.protocol = null;

                /**
                 * EventEnvelope sequenceId.
                 * @member {number} sequenceId
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.sequenceId = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * EventEnvelope taskId.
                 * @member {string} taskId
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.taskId = "";

                /**
                 * EventEnvelope eventType.
                 * @member {string} eventType
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.eventType = "";

                /**
                 * EventEnvelope payload.
                 * @member {Uint8Array} payload
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.payload = $util.newBuffer([]);

                /**
                 * EventEnvelope coreInstanceId.
                 * @member {string} coreInstanceId
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.coreInstanceId = "";

                /**
                 * EventEnvelope sessionEpoch.
                 * @member {number} sessionEpoch
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.sessionEpoch = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * EventEnvelope ready.
                 * @member {evohime.desktop.v1.Ready.$Properties|null|undefined} ready
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.ready = null;

                /**
                 * EventEnvelope replayGap.
                 * @member {evohime.desktop.v1.ReplayGap.$Properties|null|undefined} replayGap
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.replayGap = null;

                /**
                 * EventEnvelope fullSnapshot.
                 * @member {evohime.desktop.v1.FullSnapshot.$Properties|null|undefined} fullSnapshot
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.fullSnapshot = null;

                /**
                 * EventEnvelope authChallenge.
                 * @member {evohime.desktop.v1.AuthChallenge.$Properties|null|undefined} authChallenge
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                EventEnvelope.prototype.authChallenge = null;

                // OneOf field names bound to virtual getters and setters
                let $oneOfFields;

                /**
                 * EventEnvelope event.
                 * @member {"ready"|"replayGap"|"fullSnapshot"|"authChallenge"|undefined} event
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @instance
                 */
                $Object.defineProperty(EventEnvelope.prototype, "event", {
                    get: $util.oneOfGetter($oneOfFields = ["ready", "replayGap", "fullSnapshot", "authChallenge"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Encodes the specified EventEnvelope message. Does not implicitly {@link evohime.desktop.v1.EventEnvelope.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @static
                 * @param {evohime.desktop.v1.EventEnvelope.$Properties} message EventEnvelope message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                EventEnvelope.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.protocol != null && $Object.hasOwnProperty.call(message, "protocol"))
                        $root.evohime.desktop.v1.ProtocolVersion.encode(message.protocol, writer.uint32(/* id 1, wireType 2 =*/10).fork(), _depth + 1).ldelim();
                    if (message.sequenceId != null && $Object.hasOwnProperty.call(message, "sequenceId") && (typeof message.sequenceId === "object" ? message.sequenceId.low || message.sequenceId.high : message.sequenceId !== 0))
                        writer.uint32(/* id 2, wireType 0 =*/16).uint64(message.sequenceId);
                    if (message.taskId != null && $Object.hasOwnProperty.call(message, "taskId") && message.taskId !== "")
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.taskId);
                    if (message.eventType != null && $Object.hasOwnProperty.call(message, "eventType") && message.eventType !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.eventType);
                    if (message.payload != null && $Object.hasOwnProperty.call(message, "payload") && message.payload.length)
                        writer.uint32(/* id 5, wireType 2 =*/42).bytes(message.payload);
                    if (message.coreInstanceId != null && $Object.hasOwnProperty.call(message, "coreInstanceId") && message.coreInstanceId !== "")
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.coreInstanceId);
                    if (message.sessionEpoch != null && $Object.hasOwnProperty.call(message, "sessionEpoch") && (typeof message.sessionEpoch === "object" ? message.sessionEpoch.low || message.sessionEpoch.high : message.sessionEpoch !== 0))
                        writer.uint32(/* id 7, wireType 0 =*/56).uint64(message.sessionEpoch);
                    if (message.ready != null && $Object.hasOwnProperty.call(message, "ready"))
                        $root.evohime.desktop.v1.Ready.encode(message.ready, writer.uint32(/* id 10, wireType 2 =*/82).fork(), _depth + 1).ldelim();
                    if (message.replayGap != null && $Object.hasOwnProperty.call(message, "replayGap"))
                        $root.evohime.desktop.v1.ReplayGap.encode(message.replayGap, writer.uint32(/* id 11, wireType 2 =*/90).fork(), _depth + 1).ldelim();
                    if (message.fullSnapshot != null && $Object.hasOwnProperty.call(message, "fullSnapshot"))
                        $root.evohime.desktop.v1.FullSnapshot.encode(message.fullSnapshot, writer.uint32(/* id 12, wireType 2 =*/98).fork(), _depth + 1).ldelim();
                    if (message.authChallenge != null && $Object.hasOwnProperty.call(message, "authChallenge"))
                        $root.evohime.desktop.v1.AuthChallenge.encode(message.authChallenge, writer.uint32(/* id 13, wireType 2 =*/106).fork(), _depth + 1).ldelim();
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes an EventEnvelope message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.EventEnvelope & evohime.desktop.v1.EventEnvelope.$Shape} EventEnvelope
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                EventEnvelope.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.EventEnvelope(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 2)
                                    break;
                                message.protocol = $root.evohime.desktop.v1.ProtocolVersion.decode(reader, reader.uint32(), $undefined, _depth + 1, message.protocol);
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.sequenceId = value;
                                else
                                    delete message.sequenceId;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.taskId = value;
                                else
                                    delete message.taskId;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.eventType = value;
                                else
                                    delete message.eventType;
                                continue;
                            }
                        case 5: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.bytes()).length)
                                    message.payload = value;
                                else
                                    delete message.payload;
                                continue;
                            }
                        case 6: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.coreInstanceId = value;
                                else
                                    delete message.coreInstanceId;
                                continue;
                            }
                        case 7: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.sessionEpoch = value;
                                else
                                    delete message.sessionEpoch;
                                continue;
                            }
                        case 10: {
                                if (wireType !== 2)
                                    break;
                                message.ready = $root.evohime.desktop.v1.Ready.decode(reader, reader.uint32(), $undefined, _depth + 1, message.ready);
                                message.event = "ready";
                                continue;
                            }
                        case 11: {
                                if (wireType !== 2)
                                    break;
                                message.replayGap = $root.evohime.desktop.v1.ReplayGap.decode(reader, reader.uint32(), $undefined, _depth + 1, message.replayGap);
                                message.event = "replayGap";
                                continue;
                            }
                        case 12: {
                                if (wireType !== 2)
                                    break;
                                message.fullSnapshot = $root.evohime.desktop.v1.FullSnapshot.decode(reader, reader.uint32(), $undefined, _depth + 1, message.fullSnapshot);
                                message.event = "fullSnapshot";
                                continue;
                            }
                        case 13: {
                                if (wireType !== 2)
                                    break;
                                message.authChallenge = $root.evohime.desktop.v1.AuthChallenge.decode(reader, reader.uint32(), $undefined, _depth + 1, message.authChallenge);
                                message.event = "authChallenge";
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for EventEnvelope
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.EventEnvelope
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                EventEnvelope.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.EventEnvelope";
                };

                return EventEnvelope;
            })();

            v1.ReplayGap = (function() {

                /**
                 * Properties of a ReplayGap.
                 * @typedef {Object} evohime.desktop.v1.ReplayGap.$Properties
                 * @property {number|null} [requestedAfterSequence] ReplayGap requestedAfterSequence
                 * @property {number|null} [earliestAvailableSequence] ReplayGap earliestAvailableSequence
                 * @property {number|null} [latestAvailableSequence] ReplayGap latestAvailableSequence
                 * @property {string|null} [reason] ReplayGap reason
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a ReplayGap.
                 * @memberof evohime.desktop.v1
                 * @interface IReplayGap
                 * @augments evohime.desktop.v1.ReplayGap.$Properties
                 * @deprecated Use evohime.desktop.v1.ReplayGap.$Properties instead.
                 */

                /**
                 * Shape of a ReplayGap.
                 * @typedef {evohime.desktop.v1.ReplayGap.$Properties} evohime.desktop.v1.ReplayGap.$Shape
                 */

                /**
                 * Constructs a new ReplayGap.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a ReplayGap.
                 * @constructor
                 * @param {evohime.desktop.v1.ReplayGap.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const ReplayGap = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * ReplayGap requestedAfterSequence.
                 * @member {number} requestedAfterSequence
                 * @memberof evohime.desktop.v1.ReplayGap
                 * @instance
                 */
                ReplayGap.prototype.requestedAfterSequence = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * ReplayGap earliestAvailableSequence.
                 * @member {number} earliestAvailableSequence
                 * @memberof evohime.desktop.v1.ReplayGap
                 * @instance
                 */
                ReplayGap.prototype.earliestAvailableSequence = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * ReplayGap latestAvailableSequence.
                 * @member {number} latestAvailableSequence
                 * @memberof evohime.desktop.v1.ReplayGap
                 * @instance
                 */
                ReplayGap.prototype.latestAvailableSequence = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * ReplayGap reason.
                 * @member {string} reason
                 * @memberof evohime.desktop.v1.ReplayGap
                 * @instance
                 */
                ReplayGap.prototype.reason = "";

                /**
                 * Encodes the specified ReplayGap message. Does not implicitly {@link evohime.desktop.v1.ReplayGap.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.ReplayGap
                 * @static
                 * @param {evohime.desktop.v1.ReplayGap.$Properties} message ReplayGap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                ReplayGap.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.requestedAfterSequence != null && $Object.hasOwnProperty.call(message, "requestedAfterSequence") && (typeof message.requestedAfterSequence === "object" ? message.requestedAfterSequence.low || message.requestedAfterSequence.high : message.requestedAfterSequence !== 0))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.requestedAfterSequence);
                    if (message.earliestAvailableSequence != null && $Object.hasOwnProperty.call(message, "earliestAvailableSequence") && (typeof message.earliestAvailableSequence === "object" ? message.earliestAvailableSequence.low || message.earliestAvailableSequence.high : message.earliestAvailableSequence !== 0))
                        writer.uint32(/* id 2, wireType 0 =*/16).uint64(message.earliestAvailableSequence);
                    if (message.latestAvailableSequence != null && $Object.hasOwnProperty.call(message, "latestAvailableSequence") && (typeof message.latestAvailableSequence === "object" ? message.latestAvailableSequence.low || message.latestAvailableSequence.high : message.latestAvailableSequence !== 0))
                        writer.uint32(/* id 3, wireType 0 =*/24).uint64(message.latestAvailableSequence);
                    if (message.reason != null && $Object.hasOwnProperty.call(message, "reason") && message.reason !== "")
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.reason);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a ReplayGap message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.ReplayGap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReplayGap & evohime.desktop.v1.ReplayGap.$Shape} ReplayGap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                ReplayGap.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.ReplayGap(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.requestedAfterSequence = value;
                                else
                                    delete message.requestedAfterSequence;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.earliestAvailableSequence = value;
                                else
                                    delete message.earliestAvailableSequence;
                                continue;
                            }
                        case 3: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.latestAvailableSequence = value;
                                else
                                    delete message.latestAvailableSequence;
                                continue;
                            }
                        case 4: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.stringVerify()).length)
                                    message.reason = value;
                                else
                                    delete message.reason;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for ReplayGap
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.ReplayGap
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                ReplayGap.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.ReplayGap";
                };

                return ReplayGap;
            })();

            v1.FullSnapshot = (function() {

                /**
                 * Properties of a FullSnapshot.
                 * @typedef {Object} evohime.desktop.v1.FullSnapshot.$Properties
                 * @property {number|null} [sequenceId] FullSnapshot sequenceId
                 * @property {Uint8Array|null} [snapshotJson] FullSnapshot snapshotJson
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */

                /**
                 * Properties of a FullSnapshot.
                 * @memberof evohime.desktop.v1
                 * @interface IFullSnapshot
                 * @augments evohime.desktop.v1.FullSnapshot.$Properties
                 * @deprecated Use evohime.desktop.v1.FullSnapshot.$Properties instead.
                 */

                /**
                 * Shape of a FullSnapshot.
                 * @typedef {evohime.desktop.v1.FullSnapshot.$Properties} evohime.desktop.v1.FullSnapshot.$Shape
                 */

                /**
                 * Constructs a new FullSnapshot.
                 * @memberof evohime.desktop.v1
                 * @classdesc Represents a FullSnapshot.
                 * @constructor
                 * @param {evohime.desktop.v1.FullSnapshot.$Properties=} [properties] Properties to set
                 * @property {Array.<Uint8Array>} [$unknowns] Unknown fields preserved while decoding when enabled
                 */
                const FullSnapshot = function (properties) {
                    if (properties)
                        for (let keys = $Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null && keys[i] !== "__proto__")
                                this[keys[i]] = properties[keys[i]];
                };

                /**
                 * FullSnapshot sequenceId.
                 * @member {number} sequenceId
                 * @memberof evohime.desktop.v1.FullSnapshot
                 * @instance
                 */
                FullSnapshot.prototype.sequenceId = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * FullSnapshot snapshotJson.
                 * @member {Uint8Array} snapshotJson
                 * @memberof evohime.desktop.v1.FullSnapshot
                 * @instance
                 */
                FullSnapshot.prototype.snapshotJson = $util.newBuffer([]);

                /**
                 * Encodes the specified FullSnapshot message. Does not implicitly {@link evohime.desktop.v1.FullSnapshot.verify|verify} messages.
                 * @function encode
                 * @memberof evohime.desktop.v1.FullSnapshot
                 * @static
                 * @param {evohime.desktop.v1.FullSnapshot.$Properties} message FullSnapshot message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                FullSnapshot.encode = function (message, writer, _depth) {
                    if (!writer)
                        writer = $Writer.create();
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $util.recursionLimit)
                        throw $Error("max depth exceeded");
                    if (message.sequenceId != null && $Object.hasOwnProperty.call(message, "sequenceId") && (typeof message.sequenceId === "object" ? message.sequenceId.low || message.sequenceId.high : message.sequenceId !== 0))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.sequenceId);
                    if (message.snapshotJson != null && $Object.hasOwnProperty.call(message, "snapshotJson") && message.snapshotJson.length)
                        writer.uint32(/* id 2, wireType 2 =*/18).bytes(message.snapshotJson);
                    if (message.$unknowns != null && $Object.hasOwnProperty.call(message, "$unknowns"))
                        for (let i = 0; i < message.$unknowns.length; ++i)
                            writer.raw(message.$unknowns[i]);
                    return writer;
                };

                /**
                 * Decodes a FullSnapshot message from the specified reader or buffer.
                 * @function decode
                 * @memberof evohime.desktop.v1.FullSnapshot
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.FullSnapshot & evohime.desktop.v1.FullSnapshot.$Shape} FullSnapshot
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                FullSnapshot.decode = function (reader, length, _end, _depth, _target) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    if (_depth === $undefined)
                        _depth = 0;
                    if (_depth > $Reader.recursionLimit)
                        throw $Error("max depth exceeded");
                    let end = length === $undefined ? reader.len : reader.pos + length, message = _target || new $root.evohime.desktop.v1.FullSnapshot(), value;
                    while (reader.pos < end) {
                        let start = reader.pos;
                        let tag = reader.tag();
                        if (tag === _end) {
                            _end = $undefined;
                            break;
                        }
                        let wireType = tag & 7;
                        switch (tag >>>= 3) {
                        case 1: {
                                if (wireType !== 0)
                                    break;
                                if (typeof (value = reader.uint64()) === "object" ? value.low || value.high : value !== 0)
                                    message.sequenceId = value;
                                else
                                    delete message.sequenceId;
                                continue;
                            }
                        case 2: {
                                if (wireType !== 2)
                                    break;
                                if ((value = reader.bytes()).length)
                                    message.snapshotJson = value;
                                else
                                    delete message.snapshotJson;
                                continue;
                            }
                        }
                        reader.skipType(wireType, _depth, tag);
                        if (!reader.discardUnknown) {
                            $util.makeProp(message, "$unknowns", false);
                            (message.$unknowns || (message.$unknowns = [])).push(reader.raw(start, reader.pos));
                        }
                    }
                    if (_end !== $undefined)
                        throw $Error("missing end group");
                    return message;
                };

                /**
                 * Gets the type url for FullSnapshot
                 * @function getTypeUrl
                 * @memberof evohime.desktop.v1.FullSnapshot
                 * @static
                 * @param {string} [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns {string} The type url
                 */
                FullSnapshot.getTypeUrl = function(prefix) {
                    if (prefix === $undefined)
                        prefix = "type.googleapis.com";
                    return prefix + "/evohime.desktop.v1.FullSnapshot";
                };

                return FullSnapshot;
            })();

            return v1;
        })();

        return desktop;
    })();

    return evohime;
})();

export {
  $root as default
};
