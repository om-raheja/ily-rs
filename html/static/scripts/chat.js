var Chat = {
	socket: null,

	loading: document.getElementById("loading"),
	chat_box: document.getElementById("chat-box"),
	msgs_list: document.getElementById("msgs"),
	typing_list: document.getElementById("typing"),
	users: document.getElementById("users"),
	textarea: document.getElementById("form_input"),
	send_btn: document.getElementById("send"),

    // login infra
    login_btn: document.getElementById("login"),
    username_input: document.getElementById("username"),
    password_input: document.getElementById("password"),

    // just store the logins in an easy to access place
    my_nick: localStorage.nick || sessionStorage.nick || null,
    my_password: localStorage.password || sessionStorage.password || null,

	is_focused: false,
	is_online: false,
	is_typing: false,

	last_sent_nick: null,
    last_msg_time: null,


	original_title: document.title,
	new_title: "New messages...",

    // older messages handling
    is_loading: false,
    earliest_message_id: null,
    at_top: false,

	scroll: function(){
		setTimeout(function(){
			Chat.chat_box.scrollTop = Chat.chat_box.scrollHeight;
		}, 0)
	},

	notif: {
		enabled: true,

		toggle: function(){
			return Chat.notif.enabled = !Chat.notif.enabled;
		},

		// Title time-out
		ttout: undefined,

		active: undefined,
		msgs: 0,

		// Beep notification
		beep: undefined,
		beep_create: function(){
			var audiotypes = {
				"mp3": "audio/mpeg",
				"mp4": "audio/mp4",
				"ogg": "audio/ogg",
				"wav": "audio/wav"
			};

			var audios = [
				'static/beep.ogg'
			];

			var audio_element = document.createElement('audio');
			if(audio_element.canPlayType){
				for(var i = 0;i < audios.length;i++){
					var source_element = document.createElement('source');
					source_element.setAttribute('src', audios[i]);
					if(audios[i].match(/\.(\w+)$/i)){
						source_element.setAttribute('type', audiotypes[RegExp.$1]);
					}
					audio_element.appendChild(source_element);
				}

				audio_element.load();
				audio_element.playclip = function(){
					audio_element.pause();
					audio_element.volume = 0.5;
					audio_element.currentTime = 0;
					audio_element.play();
				};

				return audio_element;
			}
		},

		// Create new notification
		create: function(from, message){
            // If it's me then no notifs
            if(from === my_nick) {
                return;
            }

			// If is focused, no notification
			if(Chat.is_focused || !Chat.notif.enabled){
				return;
			}

			// Increase number in title
			Chat.notif.msgs++;

			// Create new ttout, if there is not any
			Chat.notif.favicon('blue');
			document.title = '(' + Chat.notif.msgs + ') ' + Chat.new_title;

			if(typeof Chat.notif.ttout === "undefined"){
				Chat.notif.ttout = setInterval(function(){
					if(document.title == Chat.original_title){
						Chat.notif.favicon('blue');
						document.title = '(' + Chat.notif.msgs + ') ' + Chat.new_title;
					} else {
						Chat.notif.favicon('green');
						document.title = Chat.original_title;
					}
				}, 1500);
			}

			// Do beep
			Chat.notif.beep.playclip();

			// If are'nt allowed notifications
			if(Notification.permission !== "granted"){
				Notification.requestPermission();
				return;
			}

			// Clear notification
			Chat.notif.clear();

			// Strip tags
			from = from.replace(/(<([^>]+)>)/ig, "");
			message = message.text?.replace(/(<([^>]+)>)/ig, "");

			// Create new notification
			Chat.notif.active = new Notification(from, {
				icon: 'static/images/favicon-blue.png',
				//timeout: 10,
				body: message,
			});

			// On click, focus this window
			Chat.notif.active.onclick = function(){
				parent.focus();
				window.focus();
			};
		},

		// Clear notification
		clear: function(){
			typeof Chat.notif.active === "undefined" || Chat.notif.active.close();
		},

		favicon: function(color){
			var link = document.querySelector("link[rel*='icon']") || document.createElement('link');
			link.type = 'image/x-icon';
			link.rel = 'shortcut icon';
			link.href = 'static/images/favicon-' + color + '.ico';
			document.getElementsByTagName('head')[0].appendChild(link);
		}
	},

	send_msg: function(text){
		Chat.socket.emit("send-msg", {
			m: text
		});
	},

	send_event: function(){
		var value = Chat.textarea.value.trim();
		if(value == "") return;

		console.log("Send message.");

		Chat.send_msg({text: value});

		Chat.textarea.value = '';
		Chat.typing.update();
		Chat.textarea.focus();
	},

	typing: {
		objects: {},

		create: function(nick){
			var li = document.createElement('li');

			var prefix = document.createElement('span');
			prefix.className = 'prefix';
			prefix.innerText = nick;
			li.appendChild(prefix);

			var msg = document.createElement('div');
			msg.className = 'message';

			var body = document.createElement('span');
			body.className = 'body writing'
			body.innerHTML = '<span class="one">&bull;</span><span class="two">&bull;</span><span class="three">&bull;</span>';
			msg.appendChild(body);

			li.appendChild(msg);

			Chat.typing_list.appendChild(li);

			Chat.typing.objects[nick] = li;

			// Scroll to new message
			Chat.scroll();
		},

		remove: function(nick){
			if(Chat.typing.objects.hasOwnProperty(nick)){
				var element = Chat.typing.objects[nick];
				element.parentNode.removeChild(element);
				delete Chat.typing.objects[nick];
			}
		},

		event: function(r){
			if(r.status){
				Chat.typing.create(r.nick);
			} else {
				Chat.typing.remove(r.nick);
			}
		},

		update: function(){
			if(Chat.is_typing && Chat.textarea.value === ""){
				Chat.socket.emit("typing", Chat.is_typing = false);
			}

			if(!Chat.is_typing && Chat.textarea.value !== ""){
				Chat.socket.emit("typing", Chat.is_typing = true);
			}
		}
	},

    login: function() {
        const username = Chat.username_input.value.trim();
        const password = Chat.password_input.value.trim();
        
        // Clear previous errors
        const errorEl = this.querySelector('.error');
        if(errorEl) errorEl.remove();

        Chat.socket.emit("login", {
            nick: username,
            password: password
        });
        
        // Store username in sessionStorage
        my_nick = sessionStorage.nick = localStorage.nick = username; 
        my_password = sessionStorage.password = localStorage.password = password;
    },

    new_msg: function(r, notif=true){
        const fromSelf = my_nick == r.f;

        var li = document.createElement('div');
        li.id = r.id;

        // Create header container
        var header = document.createElement('div');
        header.className = 'message-header';

        // Username
        var prefix = document.createElement('span');
        prefix.className = 'prefix';
        prefix.innerText = r.f;
        
        // Timestamp
        var time = document.createElement('span');
        time.className = 'message-time';
        time.textContent = Chat.format_time(r.time);

        header.appendChild(prefix);
        header.appendChild(time);
        li.appendChild(header);

        // Determine if header should show
        const prevNick = Chat.last_sent_nick;
        const prevTime = Chat.last_msg_time;
        const timeDiff = prevTime ? (new Date(r.time) - new Date(prevTime)) : Infinity;
        const showHeader = prevNick !== r.f || timeDiff > 600000; // 10 minutes

        header.style.display = showHeader ? "flex" : "none";
        if (showHeader) {
            Chat.last_sent_nick = r.f;
            Chat.last_msg_time = r.time;
        } else {
            li.header = header; // Store reference for potential updates
        }

        // Always update last message time
        Chat.last_msg_time = r.time;

        var msg = document.createElement('div');
        msg.className = 'message';

        var body = document.createElement('span');
        body.className = 'body' + (fromSelf ? ' out' : ' in');
        Chat.append_msg(body, r.m);

        msg.appendChild(body);
        li.appendChild(msg);

        var c = document.createElement('li');
        c.appendChild(li);
        if (fromSelf){
            c.classList.add('message-from-self');
        }

        // Prepend because flex-direction: column-reverse
        Chat.msgs_list.prepend(c);

        // Notify user
        if(notif) Chat.notif.create(r.f, r.m);

        // Scroll to new message
        Chat.scroll();
    },

    make_historical_msg_element: function(r, previousData) {
        const fromSelf = my_nick == r.f;
        const prevNick = previousData?.nick;
        const prevTime = previousData?.time;
        
        const timeDiff = prevTime ? (new Date(r.time) - new Date(prevTime)) : Infinity;
        const showHeader = prevNick !== r.f || timeDiff > 600000;

        var li = document.createElement('div');
        li.id = r.id;

        // Header container
        var header = document.createElement('div');
        header.className = 'message-header';
        header.style.display = showHeader ? "flex" : "none";

        // Username and timestamp
        var prefix = document.createElement('span');
        prefix.className = 'prefix';
        prefix.innerText = r.f;

        var time = document.createElement('span');
        time.className = 'message-time';
        time.textContent = Chat.format_time(r.time);

        header.appendChild(prefix);
        header.appendChild(time);
        li.appendChild(header);

        // Message body
        var msg = document.createElement('div');
        msg.className = 'message';

        var body = document.createElement('span');
        body.className = 'body' + (fromSelf ? ' out' : ' in');
        Chat.append_msg(body, r.m);

        msg.appendChild(body);
        li.appendChild(msg);

        var c = document.createElement('li');
        c.appendChild(li);
        if (fromSelf) {
            c.classList.add('message-from-self');
        }

        return {
            element: c,
            currentData: {
                nick: r.f,
                time: r.time
            }
        };
    },

    format_time: function(timestamp) {
        const date = new Date(timestamp);
        return `${date.toLocaleDateString(undefined, {
            month: 'numeric',
            day: 'numeric',
            year: 'numeric',
            hour12: false
        })} ${date.toLocaleTimeString(undefined, {
            hour: '2-digit',
            minute: '2-digit',
            hour12: true
        })}`;
    },

	append_msg: function(el, msg){
		if(!msg) return;

		// If is object
		if(typeof msg.text !== 'undefined'){
			// Escape HTML
			el.innerText = msg.text;
			var text = el.innerHTML;

			// Parse urls
			text = text.replace(/(https?:\/\/[^\s]+)/g, function(url, a, b){
				var link = document.createElement('a');
				link.target = "_blank";

				// Un-escape
				link.innerHTML = url;
				url = link.innerText;
				link.href = url;

				// If link is image
				if(url.match(/.(png|jpe?g|gifv?)([?#].*)?$/g)){
					var img = document.createElement('img');
					img.style = 'max-width:100%;';
					img.src = url;

					link.innerText = "";
					link.appendChild(img);
				}

				return link.outerHTML;
			});

			if(typeof Emic !== 'undefined'){
				text = Emic.replace(text);
			}

			el.innerHTML = text;
		}

		if(typeof msg.type !== 'undefined'){
			// Image
			if(msg.type.match(/image.*/)){
				var img = document.createElement('img');
				img.style = 'max-width:100%;';
				img.src = msg.url;
				el.appendChild(img);
				return;
			}

			// Audio / Video
			if(m = msg.type.match(/(audio|video).*/)){
				var audio = document.createElement(m[1]);
				audio.controls = 'controls';

				var source = document.createElement("source");
				source.src = msg.url;
				source.type = msg.type;
				audio.appendChild(source);

				el.appendChild(audio);
				return;
			}

			// Default
			var link = document.createElement('a');
			link.href = msg.url;
			link.download = msg.name;
			link.innerText = msg.name;
			el.appendChild(link);
		}
	},

	force_login: function(fail){
      //e.preventDefault();
      // Show login form if hidden
      document.getElementById('login-container').style.display = 'flex';
      document.querySelector('.chat').style.display = 'none';
      
      // Display error in form
      const form = document.getElementById('login-form');
      const errorEl = form.querySelector('.error') || document.createElement('div');
      errorEl.className = 'error';
      errorEl.style.color = '#ff4444';
      errorEl.textContent = fail;
      
      if(!form.querySelector('.error')) {
          form.insertBefore(errorEl, form.querySelector('button'));
      }
	},

	user: {
		objects: {},

		// Load all users
		start: function(r){
            document.getElementById('login-container').style.display = 'none';
            document.querySelector('.chat').style.display = '';

			Chat.users.innerText = '';

            console.log(r);
			for(var user in r.users){
				var nick = document.createElement('li');
				nick.innerText = r.users[user];
				Chat.users.appendChild(nick);
				Chat.user.objects[r.users[user]] = nick;
			}
		},

		previous_messages: function(data){
            console.log("msgs:");
			console.log(data);

            if (data.msgs.length === 0) {
                return;    
            }
            Chat.earliest_message_id = data.msgs[data.msgs.length - 1].id;
            console.log("earliest_message_id: " + Chat.earliest_message_id);

            // backend sends in descending order
			data.msgs.reverse().forEach(element => {
				Chat.new_msg(element, false);
			});
		},

		// User joined room
		enter: function(r){
			console.log("User " + r.nick + " joined.");

			var nick = document.createElement('li');
			nick.innerText = r.nick;
			Chat.users.appendChild(nick);
			Chat.user.objects[r.nick] = nick;
		},

		// User left room
		leave: function(r){
			console.log("User " + r.nick + " left.");

			// Is not typing
			Chat.typing.remove(r.nick);

			// Remove user
			if(Chat.user.objects.hasOwnProperty(r.nick)){
				var element = Chat.user.objects[r.nick];
				element.parentNode.removeChild(element);
				delete Chat.user.objects[r.nick];
			}
		}
	},

	connect: function(){
		// Set green favicon
		Chat.notif.favicon('green');
		Chat.is_online = true;

		document.getElementById('offline').style.display = "none";
		Chat.msgs_list.innerText = '';
		Chat.typing_list.innerText = '';
		Chat.users.innerText = '';
		Chat.last_sent_nick = '';

		// force user to login
		Chat.force_login();
	},

	disconnect: function(){
		// Set green favicon
		Chat.notif.favicon('red');
		Chat.is_online = false;

		document.getElementById('offline').style.display = "block";
		Chat.msgs_list.innerText = '';
		Chat.typing_list.innerText = '';
		Chat.users.innerText = '';
	},

    // tell the server to load older messages
    load_older_messages: function() {
      // Show loading indicator
      if (!Chat.is_loading) {
        const loader = document.createElement('div');
        loader.className = 'loading-older';
        loader.textContent = 'Loading older messages...';
        Chat.msgs_list.appendChild(loader);
        Chat.is_loading = true;

        // Request older messages from server
        Chat.socket.emit('load-more-messages', {
          last: Chat.earliest_message_id,
        });
      } 
    },

    // receive older messages
    older_messages: function(data) {
        console.log("received %s older messages to render", data.msgs.length);
        const loader = document.querySelector('.loading-older');
        
        Chat.is_loading = false;

        if(data.msgs.length > 0) {
            if(loader) loader.remove();
            const prevScrollHeight = Chat.chat_box.scrollHeight;
            const prevScrollTop = Chat.chat_box.scrollTop;

            // Track previous nick within this batch
            let previousData = null;
            const fragment = document.createDocumentFragment();
            
            // Process messages in reverse order (oldest first)
            data.msgs.reverse().forEach(msg => {
                const { element, currentData } = Chat.make_historical_msg_element(msg, previousData);
                fragment.prepend(element);
                previousData = currentData;
            });

            // Insert at top
            Chat.msgs_list.append(fragment);

            // Update earliest known message ID
            Chat.earliest_message_id = data.msgs[0].id;

            // Adjust scroll position
            const newScrollHeight = Chat.chat_box.scrollHeight;
            Chat.chat_box.scrollTop = prevScrollTop + (newScrollHeight - prevScrollHeight);
        } else if (data.msgs.length === 0) {
            console.log("no older messages to render");
            loader.textContent = 'No older messages to render';
            Chat.at_top = true;
        }
    },

	init: function(socket){
		// Set green favicon
		Chat.notif.favicon('red');

		// Connect to socket.io
		Chat.socket = socket || io();

		// Create beep object
		Chat.notif.beep = Chat.notif.beep_create();

        // Add scroll event listener to chat box
        Chat.chat_box.addEventListener('scroll', function() {
          // Detect when scrolled near top (100px threshold)
          if(this.scrollTop < 100 && !Chat.isLoading) {
            if (!Chat.at_top) {
              Chat.load_older_messages();
            }
          }
        });

        // Add loading state management
        Chat.is_loading = false;
        Chat.earliest_message_id = null;

		// On focus
		window.addEventListener('focus', function(){
			Chat.is_focused = true;

			// If chat is not online, dont care.
			if(!Chat.is_online){
				return;
			}

            Chat.socket.emit('user-active');

			// Clear ttout, if there was
			typeof Chat.notif.ttout === "undefined" || clearInterval(Chat.notif.ttout);
			Chat.notif.ttout = undefined;

			// Clear notifications
			Chat.notif.clear();
			Chat.notif.msgs = 0;
			Chat.notif.favicon('green');

			// Set back page title
			document.title = Chat.original_title;
            // TODO: ungrey out in active users
		});

		// On blur
		window.addEventListener('blur', function(){
			Chat.is_focused = false;
            Chat.socket.emit('user-inactive');
		});
        
        // On login 
        Chat.login_btn.onclick = Chat.login;

		// On click send message
		Chat.send_btn.onclick = Chat.send_event;

		// On enter send message
		Chat.textarea.onkeydown = function(e){
			var key = e.keyCode || window.event.keyCode;

			// If the user has pressed enter
			if(key === 13){
				Chat.send_event();
				return false;
			}

			return true;
		};

		// Check if is user typing
		Chat.textarea.onkeyup = Chat.typing.update;

		// On socket events
		Chat.socket.on("connect", Chat.connect);
		Chat.socket.on("disconnect", Chat.disconnect);

		Chat.socket.on("force-login", Chat.force_login);
		Chat.socket.on("typing", Chat.typing.event);
		Chat.socket.on("new-msg", Chat.new_msg);

		Chat.socket.on("previous-msg", Chat.user.previous_messages)
		Chat.socket.on("start", Chat.user.start);
		Chat.socket.on("ue", Chat.user.enter);
		Chat.socket.on("ul", Chat.user.leave);

        Chat.socket.on("older-msgs", Chat.older_messages);

		var dropZone = document.getElementsByTagName("body")[0];

		// Optional. Show the copy icon when dragging over. Seems to only work for chrome.
		dropZone.addEventListener('dragover', function(e){
			e.stopPropagation();
			e.preventDefault();

			e.dataTransfer.dropEffect = 'copy';
		});

		// Get file data on drop
		dropZone.addEventListener('drop', function(e){
			e.stopPropagation();
			e.preventDefault();

			var files = e.dataTransfer.files; // Array of all files
			for(var i = 0;i < files.length;i++){
				var file = files[i];

				// Max 10 MB
				if(file.size > 10485760){
					alert("Max size of file is 10MB");
					return;
				}

				var reader = new FileReader();
				reader.onload = (function(file){
					return function(e){
						Chat.send_msg({
							type: file.type,
							name: file.name,
							url: e.target.result
						});
					};
				})(file);
				reader.readAsDataURL(file);
			}
		});

		// close socket upon refresh or tab close, free the username
		window.addEventListener("beforeunload", () => {
			if(!Chat.is_online){
				return;
			}
			Chat.socket.disconnect();
		});
	}
};
