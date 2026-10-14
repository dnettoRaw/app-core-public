import {multiply,translate,rotateX,rotateY,perspective,initial,command,createLorenz,advanceLorenz,TRAILS,TRAIL_LENGTH,FIXED_STEP} from './model.mjs';
const canvas=document.querySelector('#scene'),status=document.querySelector('#status'),state=initial();let simulation=createLorenz();
let gl=null,program=null,buffer=null,projection=null,tint=null,size=null,previous=0,accumulator=0,request=0,lost=false,drag=null,hudTime=0;
const colors=[[.95,.78,.35],[.24,.79,.79],[.50,.64,.98],[.92,.41,.58],[.58,.85,.50],[.76,.50,.92]];
function shader(type,source){const item=gl.createShader(type);if(!item)throw Error('Shader unavailable');gl.shaderSource(item,source);gl.compileShader(item);if(!gl.getShaderParameter(item,gl.COMPILE_STATUS)){gl.deleteShader(item);throw Error('Shader compilation failed');}return item;}
function initialize(){
 gl=canvas.getContext('webgl',{alpha:false,antialias:true});if(!gl)throw Error('WebGL unavailable on this system');
 const vs=shader(gl.VERTEX_SHADER,'attribute vec3 p; uniform mat4 m; uniform float pointSize; varying float shade; void main(){gl_Position=m*vec4(p,1.0);gl_PointSize=pointSize;shade=0.65+0.25*clamp((p.y+2.5)/5.0,0.0,1.0);}');
 const fs=shader(gl.FRAGMENT_SHADER,'precision mediump float; uniform vec3 color; varying float shade; void main(){vec2 q=gl_PointCoord-vec2(0.5);if(dot(q,q)>0.25)discard;gl_FragColor=vec4(color*shade,0.72);}');
 program=gl.createProgram();gl.attachShader(program,vs);gl.attachShader(program,fs);gl.linkProgram(program);gl.deleteShader(vs);gl.deleteShader(fs);if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw Error('Program linking failed');
 gl.useProgram(program);buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,simulation.points,gl.DYNAMIC_DRAW);
 const position=gl.getAttribLocation(program,'p');gl.enableVertexAttribArray(position);gl.vertexAttribPointer(position,3,gl.FLOAT,false,12,0);
 projection=gl.getUniformLocation(program,'m');tint=gl.getUniformLocation(program,'color');size=gl.getUniformLocation(program,'pointSize');gl.enable(gl.DEPTH_TEST);gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
 lost=false;previous=0;accumulator=0;update();request=requestAnimationFrame(frame);
}
function upload(start,steps){
 if(!steps)return;const first=Math.min(steps,TRAIL_LENGTH-start),second=steps-first;
 gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
 for(let i=0;i<TRAILS;i++){
  const offset=(i*TRAIL_LENGTH+start)*3;gl.bufferSubData(gl.ARRAY_BUFFER,offset*4,simulation.points.subarray(offset,offset+first*3));
  if(second){const offset=i*TRAIL_LENGTH*3;gl.bufferSubData(gl.ARRAY_BUFFER,offset*4,simulation.points.subarray(offset,offset+second*3));}
 }
}
function advance(steps){const start=simulation.cursor;advanceLorenz(simulation,steps);upload(start,steps);}
function update(){
 document.querySelector('#motion').textContent=state.motion?'Pause motion':'Resume motion';document.querySelector('#explode').textContent=state.explode?'Show all trails':'Isolate trail';
 document.querySelector('#selection').textContent=`Trail ${state.selected+1} of ${TRAILS} · seed Δx = ${(state.selected*.001).toFixed(3)} · command revision ${state.revision}`;
 status.textContent=`${state.motion?'Simulating':'Paused'} · ${simulation.count*TRAILS} retained points · t = ${(simulation.steps*FIXED_STEP).toFixed(2)}`;
}
for(const id of ['motion','select','explode','reset','near','far','step'])document.querySelector('#'+id).onclick=()=>{
 command(state,id);
 if(id==='reset'){simulation=createLorenz();accumulator=0;gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferSubData(gl.ARRAY_BUFFER,0,simulation.points);}
 if(id==='step')advance(8);update();
};
canvas.addEventListener('pointerdown',e=>{drag=[e.clientX,e.clientY];canvas.setPointerCapture(e.pointerId);});canvas.addEventListener('pointermove',e=>{if(!drag)return;state.yaw+=(e.clientX-drag[0])*.008;state.pitch=Math.max(-1.3,Math.min(1.3,state.pitch+(e.clientY-drag[1])*.008));state.revision++;drag=[e.clientX,e.clientY];});for(const event of ['pointerup','pointercancel','lostpointercapture'])canvas.addEventListener(event,()=>{drag=null;});
document.addEventListener('visibilitychange',()=>{previous=0;accumulator=0;});
function frame(now){
 if(lost)return;const dt=previous?Math.min(.04,(now-previous)/1000):0;previous=now;
 if(state.motion&&!document.hidden){accumulator=Math.min(.04,accumulator+dt);const steps=Math.min(8,Math.floor((accumulator+1e-9)/FIXED_STEP));if(steps){advance(steps);accumulator-=steps*FIXED_STEP;}}
 const dpr=Math.min(2,window.devicePixelRatio||1),width=Math.max(1,Math.min(1920,Math.round(canvas.clientWidth*dpr))),height=Math.round(width*420/960);
 if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}gl.viewport(0,0,width,height);gl.clearColor(.045,.075,.11,1);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
 const view=multiply(multiply(translate(0,0,-state.distance),rotateX(state.pitch)),rotateY(state.yaw));gl.uniformMatrix4fv(projection,false,multiply(perspective(width/height),view));
 for(let i=0;i<TRAILS;i++){
  if(state.explode&&i!==state.selected)continue;
  gl.uniform3fv(tint,colors[i]);gl.uniform1f(size,(i===state.selected?2.5:1.6)*dpr);gl.drawArrays(gl.POINTS,i*TRAIL_LENGTH,simulation.count);
  gl.uniform3fv(tint,[1,1,1]);gl.uniform1f(size,7*dpr);gl.drawArrays(gl.POINTS,i*TRAIL_LENGTH+(simulation.cursor+TRAIL_LENGTH-1)%TRAIL_LENGTH,1);
 }
 if(now-hudTime>150){update();hudTime=now;}request=requestAnimationFrame(frame);
}
canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();lost=true;cancelAnimationFrame(request);status.textContent='Context lost — waiting for system recovery';document.querySelectorAll('button').forEach(b=>b.disabled=true);});
canvas.addEventListener('webglcontextrestored',()=>{try{document.querySelectorAll('button').forEach(b=>b.disabled=false);initialize();}catch(error){fail(error);}});
function fail(error){status.textContent=error.message;document.querySelectorAll('button').forEach(b=>b.disabled=true);}
window.addEventListener('pagehide',()=>{lost=true;cancelAnimationFrame(request);if(gl&&!gl.isContextLost()){gl.deleteBuffer(buffer);gl.deleteProgram(program);}});
try{initialize();}catch(error){fail(error);}
